import importlib.util
import io
import json
import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SPEC = importlib.util.spec_from_file_location("import_linear", Path(__file__).with_name("import_linear.py"))
IMPORTER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IMPORTER)


def backlog():
    return IMPORTER.validate_backlog({"project_name": "MCPTools", "initiative": "atlas-redesign-2026-09-08", "issues": [
        {"key": "ATLAS-02", "title": "child", "description": "child", "priority": 2, "parent": "ATLAS-01", "depends_on": ["ATLAS-00"]},
        {"key": "ATLAS-00", "title": "base", "description": "base", "priority": 1, "parent": None, "depends_on": []},
        {"key": "ATLAS-01", "title": "parent", "description": "parent", "priority": 2, "parent": None, "depends_on": ["ATLAS-00"]},
    ]})


class FakeClient:
    def __init__(self, existing=None, error=False, projects=None, mutation_error=False):
        self.existing = existing or []
        self.error = error
        self.mutation_error = mutation_error
        self.projects = projects or [{"id": "project", "name": "MCPTools", "teams": {"nodes": [{"id": "team", "name": "Core"}]}}]
        self.created = []
        self.relations = []

    def request(self, operation, variables):
        if self.error:
            raise IMPORTER.GraphQLError("Linear GraphQL error: denied")
        if operation == IMPORTER.PROJECTS:
            return {"projects": {"nodes": self.projects, "pageInfo": {"hasNextPage": False, "endCursor": None}}}
        if operation == IMPORTER.PROJECT:
            return {"project": self.projects[0]}
        if operation == IMPORTER.PROJECT_TEAMS:
            return {"project": {"teams": {"nodes": self.projects[0].get("teams", {}).get("nodes", []), "pageInfo": {"hasNextPage": False, "endCursor": None}}}}
        if operation == IMPORTER.ISSUES:
            return {"project": {"issues": {"nodes": self.existing, "pageInfo": {"hasNextPage": False, "endCursor": None}}}}
        if operation == IMPORTER.ISSUE:
            issue = next((item for item in self.existing if item["id"] == variables["id"]), None)
            return {"issue": None if issue is None else {"id": issue["id"], "url": issue.get("url"), "project": {"id": "project"}}}
        if operation == IMPORTER.CREATE_ISSUE:
            if self.mutation_error:
                raise IMPORTER.GraphQLError("Linear GraphQL error: denied")
            self.created.append(variables["input"])
            number = len(self.created)
            created = {"id": "issue-%d" % number, "url": "https://linear/issue-%d" % number, "description": variables["input"]["description"], "relations": {"nodes": [], "pageInfo": {"hasNextPage": False}}}
            self.existing.append(created)
            return {"issueCreate": {"success": True, "issue": {"id": created["id"], "url": created["url"]}}}
        if operation == IMPORTER.CREATE_RELATION:
            if self.mutation_error:
                raise IMPORTER.GraphQLError("Linear GraphQL error: denied")
            self.relations.append(variables["input"])
            source = next(item for item in self.existing if item["id"] == variables["input"]["issueId"])
            source["relations"]["nodes"].append({"type": "blocks", "relatedIssue": {"id": variables["input"]["relatedIssueId"]}})
            return {"issueRelationCreate": {"success": True, "issueRelation": {"id": "relation-%d" % len(self.relations)}}}
        raise AssertionError(operation)


class ImporterTests(unittest.TestCase):
    def test_validation_rejects_cycles_and_unknown_references(self):
        value = {"project_name": "MCPTools", "initiative": "i", "issues": [{"key": "A", "title": "A", "description": "", "priority": 1, "parent": None, "depends_on": ["B"]}, {"key": "B", "title": "B", "description": "", "priority": 1, "parent": None, "depends_on": ["A"]}]}
        with self.assertRaisesRegex(IMPORTER.ValidationError, "cycle"):
            IMPORTER.validate_backlog(value)
        value["issues"][0]["depends_on"] = ["missing"]
        with self.assertRaisesRegex(IMPORTER.ValidationError, "unknown"):
            IMPORTER.validate_backlog(value)
        value["issues"][0]["depends_on"] = []
        value["issues"][0]["priority"] = True
        with self.assertRaisesRegex(IMPORTER.ValidationError, "priority"):
            IMPORTER.validate_backlog(value)

    def test_apply_creates_parent_and_prerequisite_before_child(self):
        client = FakeClient()
        with tempfile.TemporaryDirectory() as directory:
            result = IMPORTER.apply(backlog(), client, receipt_path=Path(directory) / "receipt.json")
        self.assertEqual([item["title"] for item in client.created], ["base", "parent", "child"])
        self.assertEqual(client.created[2]["parentId"], "issue-2")
        self.assertEqual([(item["issueId"], item["relatedIssueId"]) for item in client.relations], [("issue-1", "issue-2"), ("issue-1", "issue-3")])
        self.assertEqual(len(result["receipt"]["issues"]), 3)

    def test_rerun_uses_remote_markers_without_new_mutations(self):
        client = FakeClient()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            IMPORTER.apply(backlog(), client, receipt_path=path)
            path.unlink()
            created = len(client.created)
            relations = len(client.relations)
            IMPORTER.apply(backlog(), client, receipt_path=path)
        self.assertEqual(len(client.created), created)
        self.assertEqual(len(client.relations), relations)

    def test_graphql_errors_and_ambiguous_project_stop_import(self):
        with self.assertRaisesRegex(IMPORTER.GraphQLError, "denied"):
            IMPORTER.apply(backlog(), FakeClient(error=True))
        client = FakeClient(projects=[{"id": "one", "name": "mcptools", "teams": {"nodes": []}}, {"id": "two", "name": "MCPTOOLS", "teams": {"nodes": []}}])
        with self.assertRaisesRegex(IMPORTER.ImportError, "found 2"):
            IMPORTER.apply(backlog(), client)
        client = FakeClient(projects=[{"id": "project", "name": "MCPTools", "teams": {"nodes": [{"id": "a"}, {"id": "b"}]}}])
        with self.assertRaisesRegex(IMPORTER.ImportError, "use --team-id"):
            IMPORTER.apply(backlog(), client)

    def test_mismatched_receipt_target_stops_before_mutations(self):
        client = FakeClient()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            path.write_text(json.dumps({"initiative": backlog()["initiative"], "project_id": "other", "team_id": "team", "issues": {}, "dependencies": {}}), encoding="utf-8")
            with self.assertRaisesRegex(IMPORTER.ImportError, "receipt target"):
                IMPORTER.apply(backlog(), client, receipt_path=path)
        self.assertEqual(client.created, [])

    def test_graphql_payload_errors_and_mutation_errors_stop(self):
        class Response:
            def __enter__(self):
                return self

            def __exit__(self, *unused):
                return None

            def read(self, *unused):
                return json.dumps({"errors": [{"message": "denied"}]}).encode()

        with mock.patch.object(IMPORTER.urllib.request, "urlopen", return_value=Response()):
            with self.assertRaisesRegex(IMPORTER.GraphQLError, "denied"):
                IMPORTER.LinearClient("secret").request("query Test { viewer { id } }", {})
        client = FakeClient(mutation_error=True)
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(IMPORTER.UncertainMutationError, "rerun to reconcile.*denied"):
                IMPORTER.apply(backlog(), client, receipt_path=Path(directory) / "receipt.json")
        self.assertEqual(client.created, [])
        self.assertEqual(client.relations, [])

    def test_http_error_details_are_safe_and_malformed_payloads_are_safe(self):
        api_key = "linear-secret"
        error = IMPORTER.urllib.error.HTTPError(IMPORTER.API_URL, 400, "Bad Request", {}, io.BytesIO(json.dumps({"errors": [{"message": "bad input " + api_key, "extensions": {"code": "BAD_USER_INPUT"}}]}).encode()))
        with mock.patch.object(IMPORTER.urllib.request, "urlopen", side_effect=error):
            with self.assertRaises(IMPORTER.GraphQLError) as raised:
                IMPORTER.LinearClient(api_key).request(IMPORTER.CREATE_ISSUE, {})
        message = str(raised.exception)
        self.assertIn("HTTP 400", message)
        self.assertIn("IssueCreate", message)
        self.assertIn("BAD_USER_INPUT", message)
        self.assertNotIn(api_key, message)
        invalid = IMPORTER.urllib.error.HTTPError(IMPORTER.API_URL, 400, "Bad Request", {}, io.BytesIO(b"<html>broken</html>"))
        with mock.patch.object(IMPORTER.urllib.request, "urlopen", side_effect=invalid):
            with self.assertRaisesRegex(IMPORTER.GraphQLError, "invalid response body|<html>broken"):
                IMPORTER.LinearClient(api_key).request(IMPORTER.CREATE_ISSUE, {})
        class Response:
            def __enter__(self):
                return self

            def __exit__(self, *unused):
                return None

            def read(self, *unused):
                return b"[]"

        with mock.patch.object(IMPORTER.urllib.request, "urlopen", return_value=Response()):
            with self.assertRaisesRegex(IMPORTER.GraphQLError, "invalid JSON object"):
                IMPORTER.LinearClient(api_key).request(IMPORTER.CREATE_ISSUE, {})

    def test_missing_marker_receipt_and_truncated_relations_stop(self):
        missing_marker = [{"id": "issue-1", "url": "https://linear/issue-1", "description": "moved", "relations": {"nodes": [], "pageInfo": {"hasNextPage": False}}}]
        client = FakeClient(existing=missing_marker)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            path.write_text(json.dumps({"initiative": backlog()["initiative"], "project_id": "project", "team_id": "team", "issues": {"ATLAS-00": {"id": "issue-1", "url": "https://linear/issue-1"}}, "dependencies": {}}), encoding="utf-8")
            with self.assertRaisesRegex(IMPORTER.ImportError, "missing or has a changed"):
                IMPORTER.apply(backlog(), client, receipt_path=path)
        truncated = [{"id": "issue-1", "url": "https://linear/issue-1", "description": IMPORTER.marker(backlog()["initiative"], "ATLAS-00"), "relations": {"nodes": [], "pageInfo": {"hasNextPage": True}}}]
        with self.assertRaisesRegex(IMPORTER.ImportError, "truncated"):
            IMPORTER.apply(backlog(), FakeClient(existing=truncated))

    def test_issue_snapshot_uses_bounded_pages_and_preserves_relations(self):
        class PagedClient:
            def request(self, operation, variables):
                if operation != IMPORTER.ISSUES:
                    raise AssertionError(operation)
                if variables["after"] is None:
                    nodes = [{"id": "one", "url": "https://linear/one", "description": IMPORTER.marker("i", "A"), "relations": {"nodes": [{"type": "blocks", "relatedIssue": {"id": "two"}}], "pageInfo": {"hasNextPage": False}}}]
                    return {"project": {"issues": {"nodes": nodes, "pageInfo": {"hasNextPage": True, "endCursor": "next"}}}}
                nodes = [{"id": "two", "url": "https://linear/two", "description": IMPORTER.marker("i", "B"), "relations": {"nodes": [], "pageInfo": {"hasNextPage": False}}}]
                return {"project": {"issues": {"nodes": nodes, "pageInfo": {"hasNextPage": False, "endCursor": None}}}}

        found, relations, remote = IMPORTER.existing_issues(PagedClient(), "project", "i")
        issue_page = int(re.search(r"issues\(first: (\d+)", IMPORTER.ISSUES).group(1))
        relation_page = int(re.search(r"relations\(first: (\d+)", IMPORTER.ISSUES).group(1))
        self.assertLess(issue_page * relation_page * 1.1, 10000)
        self.assertEqual(set(found), {"A", "B"})
        self.assertEqual(relations, {("one", "two")})
        self.assertEqual(set(remote), {"one", "two"})


if __name__ == "__main__":
    unittest.main()
