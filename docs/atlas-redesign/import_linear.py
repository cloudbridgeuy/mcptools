import argparse
import json
import os
import re
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path


API_URL = "https://api.linear.app/graphql"


class ImportError(Exception):
    pass


class ValidationError(ImportError):
    pass


class GraphQLError(ImportError):
    pass


class UncertainMutationError(ImportError):
    pass


def operation_name(operation):
    matched = re.search(r"\b(?:query|mutation)\s+([A-Za-z_][A-Za-z0-9_]*)", operation)
    return matched.group(1) if matched else "anonymous"


def safe_text(value, api_key):
    text = str(value).replace(api_key, "[redacted]")
    return " ".join(text.split())[:500]


def graphql_messages(value, api_key):
    entries = value if isinstance(value, list) else [value]
    messages = []
    for entry in entries:
        if not isinstance(entry, dict):
            messages.append(safe_text(entry, api_key))
            continue
        message = safe_text(entry.get("message", "GraphQL error"), api_key)
        extensions = entry.get("extensions")
        code = extensions.get("code") if isinstance(extensions, dict) else None
        messages.append(message + (" (code=%s)" % safe_text(code, api_key) if code is not None else ""))
    return "; ".join(messages) or "GraphQL error"


def http_error_message(error, operation, api_key):
    status = getattr(error, "code", "unknown")
    try:
        body = error.read(8192)
    except Exception:
        body = b""
    if isinstance(body, bytes):
        text = body.decode("utf-8", "replace")
    else:
        text = str(body)
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        detail = safe_text(text, api_key) or "invalid response body"
    else:
        detail = graphql_messages(payload.get("errors"), api_key) if isinstance(payload, dict) and "errors" in payload else safe_text(text, api_key)
    return "Linear HTTP %s for %s: %s" % (status, operation_name(operation), detail)


def marker(initiative, key):
    return "[atlas-redesign-import initiative=%s key=%s]" % (initiative, key)


def validate_backlog(value):
    if not isinstance(value, dict):
        raise ValidationError("backlog must be an object")
    required = ("project_name", "initiative", "issues")
    if any(not isinstance(value.get(name), str) or not value[name].strip() for name in required[:2]):
        raise ValidationError("project_name and initiative must be non-empty strings")
    if not isinstance(value.get("issues"), list) or not value["issues"]:
        raise ValidationError("issues must be a non-empty list")
    issues = []
    keys = set()
    for item in value["issues"]:
        if not isinstance(item, dict):
            raise ValidationError("each issue must be an object")
        key = item.get("key")
        if not isinstance(key, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", key):
            raise ValidationError("issue key must contain only letters, numbers, _ and -")
        if key in keys:
            raise ValidationError("duplicate issue key: %s" % key)
        keys.add(key)
        if not isinstance(item.get("title"), str) or not item["title"].strip():
            raise ValidationError("issue %s has no title" % key)
        if not isinstance(item.get("description"), str):
            raise ValidationError("issue %s description must be a string" % key)
        if isinstance(item.get("priority"), bool) or not isinstance(item.get("priority"), int) or item["priority"] not in range(0, 5):
            raise ValidationError("issue %s priority must be an integer from 0 to 4" % key)
        parent = item.get("parent")
        depends_on = item.get("depends_on")
        if parent is not None and not isinstance(parent, str):
            raise ValidationError("issue %s parent must be null or a key" % key)
        if not isinstance(depends_on, list) or not all(isinstance(dep, str) for dep in depends_on):
            raise ValidationError("issue %s depends_on must be a list of keys" % key)
        if len(set(depends_on)) != len(depends_on):
            raise ValidationError("issue %s repeats a dependency" % key)
        if key in depends_on or parent == key:
            raise ValidationError("issue %s cannot refer to itself" % key)
        issues.append({"key": key, "title": item["title"], "description": item["description"], "priority": item["priority"], "parent": parent, "depends_on": depends_on})
    for issue in issues:
        references = issue["depends_on"] + ([] if issue["parent"] is None else [issue["parent"]])
        for reference in references:
            if reference not in keys:
                raise ValidationError("issue %s refers to unknown key %s" % (issue["key"], reference))
    ordered = order_issues(issues)
    return {"project_name": value["project_name"], "initiative": value["initiative"], "issues": issues, "ordered": ordered}


def order_issues(issues):
    by_key = {issue["key"]: issue for issue in issues}
    requirements = {key: set(issue["depends_on"] + ([] if issue["parent"] is None else [issue["parent"]])) for key, issue in by_key.items()}
    ordered = []
    while requirements:
        ready = [issue["key"] for issue in issues if issue["key"] in requirements and not requirements[issue["key"]]]
        if not ready:
            raise ValidationError("parent/dependency references contain a cycle")
        for key in ready:
            ordered.append(by_key[key])
            del requirements[key]
        for needed in requirements.values():
            needed.difference_update(ready)
    return ordered


def description_for(issue, initiative):
    dependencies = issue["depends_on"]
    suffix = "\n\n%s" % marker(initiative, issue["key"])
    if dependencies:
        suffix += "\n\nDependencies: " + ", ".join(dependencies)
    return issue["description"].rstrip() + suffix


def receipt_for(initiative, project_id, team_id):
    return {"initiative": initiative, "project_id": project_id, "team_id": team_id, "issues": {}, "dependencies": {}}


def merge_receipt(value, initiative, project_id, team_id):
    if not isinstance(value, dict):
        raise ImportError("receipt must be an object")
    if value.get("initiative") != initiative:
        raise ImportError("receipt initiative does not match this backlog")
    if value.get("project_id") != project_id or value.get("team_id") != team_id:
        raise ImportError("receipt target does not match resolved project and team")
    issues = value.get("issues") if isinstance(value.get("issues"), dict) else {}
    dependencies = value.get("dependencies") if isinstance(value.get("dependencies"), dict) else {}
    return {"initiative": initiative, "project_id": project_id, "team_id": team_id, "issues": issues, "dependencies": dependencies}


class LinearClient:
    def __init__(self, api_key):
        self.api_key = api_key

    def request(self, operation, variables):
        body = json.dumps({"query": operation, "variables": variables}).encode()
        request = urllib.request.Request(API_URL, body, {"Authorization": self.api_key, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                payload = json.load(response)
        except urllib.error.HTTPError as error:
            raise GraphQLError(http_error_message(error, operation, self.api_key)) from error
        except (urllib.error.URLError, json.JSONDecodeError) as error:
            raise GraphQLError("Linear request failed for %s: %s" % (operation_name(operation), safe_text(error, self.api_key))) from error
        if not isinstance(payload, dict):
            raise GraphQLError("Linear returned invalid JSON object for %s" % operation_name(operation))
        if payload.get("errors"):
            raise GraphQLError("Linear GraphQL error for %s: %s" % (operation_name(operation), graphql_messages(payload["errors"], self.api_key)))
        if not isinstance(payload.get("data"), dict):
            raise GraphQLError("Linear returned no GraphQL data for %s" % operation_name(operation))
        return payload["data"]


PROJECTS = "query Projects($after: String) { projects(first: 100, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } }"
PROJECT = "query Project($id: String!) { project(id: $id) { id name } }"
PROJECT_TEAMS = "query ProjectTeams($id: String!, $after: String) { project(id: $id) { teams(first: 100, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }"
ISSUES = "query ProjectIssues($id: String!, $after: String) { project(id: $id) { issues(first: 25, after: $after, includeArchived: true) { nodes { id url description relations(first: 100) { nodes { type relatedIssue { id } } pageInfo { hasNextPage } } } pageInfo { hasNextPage endCursor } } } }"
ISSUE = "query Issue($id: String!) { issue(id: $id) { id url project { id } } }"
CREATE_ISSUE = "mutation IssueCreate($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id url } } }"
CREATE_RELATION = "mutation IssueRelationCreate($input: IssueRelationCreateInput!) { issueRelationCreate(input: $input) { success issueRelation { id } } }"


def pages(client, operation, variables, connection):
    after = None
    while True:
        values = dict(variables, after=after)
        data = client.request(operation, values)
        current = data
        for name in connection:
            current = current.get(name) if isinstance(current, dict) else None
        if not isinstance(current, dict):
            raise GraphQLError("Linear response is missing %s" % ".".join(connection))
        yield current.get("nodes", [])
        page_info = current.get("pageInfo") or {}
        if not page_info.get("hasNextPage"):
            return
        after = page_info.get("endCursor")
        if not after:
            raise GraphQLError("Linear pagination has no end cursor")


def select_project(client, project_name, project_id):
    if project_id:
        data = client.request(PROJECT, {"id": project_id})
        project = data.get("project")
        if not isinstance(project, dict):
            raise ImportError("project id was not found: %s" % project_id)
        return project
    matches = []
    for nodes in pages(client, PROJECTS, {}, ("projects",)):
        matches.extend(project for project in nodes if str(project.get("name", "")).casefold() == project_name.casefold())
    if len(matches) != 1:
        raise ImportError("expected one MCPTools project match, found %d; use --project-id" % len(matches))
    return matches[0]


def select_team(client, project, team_id):
    teams = []
    for nodes in pages(client, PROJECT_TEAMS, {"id": project["id"]}, ("project", "teams")):
        teams.extend(nodes)
    if team_id:
        matches = [team for team in teams if team.get("id") == team_id]
        if len(matches) != 1:
            raise ImportError("team id is not associated with the selected project: %s" % team_id)
        return matches[0]
    if len(teams) != 1:
        raise ImportError("selected project has %d teams; use --team-id" % len(teams))
    return teams[0]


def existing_issues(client, project_id, initiative):
    found = {}
    relations = set()
    remote = {}
    expression = re.compile(r"\[atlas-redesign-import initiative=%s key=([A-Za-z0-9_-]+)\]" % re.escape(initiative))
    for nodes in pages(client, ISSUES, {"id": project_id}, ("project", "issues")):
        for issue in nodes:
            if not isinstance(issue.get("id"), str):
                raise GraphQLError("Linear issue snapshot has no id")
            remote[issue["id"]] = issue
            matches = expression.findall(issue.get("description") or "")
            if len(matches) > 1:
                raise ImportError("existing issue %s has multiple importer markers" % issue.get("id"))
            if matches:
                key = matches[0]
                if key in found:
                    raise ImportError("multiple existing issues have marker for %s" % key)
                found[key] = {"id": issue["id"], "url": issue.get("url")}
            for relation in (issue.get("relations") or {}).get("nodes") or []:
                related = relation.get("relatedIssue") if isinstance(relation, dict) else None
                if isinstance(relation, dict) and relation.get("type") == "blocks" and isinstance(related, dict) and related.get("id"):
                    relations.add((issue["id"], related["id"]))
            page_info = (issue.get("relations") or {}).get("pageInfo") or {}
            if page_info.get("hasNextPage"):
                raise ImportError("issue relation snapshot is truncated; refusing to create duplicate relations")
    return found, relations, remote


def mutation(client, operation, variables, name):
    try:
        return client.request(operation, variables)
    except GraphQLError as error:
        raise UncertainMutationError("%s outcome is uncertain; rerun to reconcile before continuing: %s" % (name, error)) from error


def atomic_write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False, encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")
        temporary = Path(output.name)
    temporary.replace(path)


def read_receipt(path, initiative, project_id, team_id):
    if not path.exists():
        return receipt_for(initiative, project_id, team_id)
    try:
        return merge_receipt(json.loads(path.read_text(encoding="utf-8")), initiative, project_id, team_id)
    except json.JSONDecodeError as error:
        raise ImportError("receipt is not valid JSON: %s" % path) from error


def apply(backlog, client, project_id=None, team_id=None, receipt_path=None):
    project = select_project(client, backlog["project_name"], project_id)
    team = select_team(client, project, team_id)
    receipt_path = Path(receipt_path) if receipt_path else Path("linear-receipt.json")
    receipt = read_receipt(receipt_path, backlog["initiative"], project["id"], team["id"])
    existing, existing_relations, remote = existing_issues(client, project["id"], backlog["initiative"])
    for key, value in receipt["issues"].items():
        issue_id = value.get("id") if isinstance(value, dict) else None
        if not isinstance(issue_id, str):
            raise ImportError("receipt issue %s has no valid id" % key)
        snapshot = remote.get(issue_id)
        if snapshot is None:
            try:
                data = client.request(ISSUE, {"id": issue_id})
            except GraphQLError as error:
                raise ImportError("receipt issue %s cannot be reconciled with the selected project" % key) from error
            issue = data.get("issue")
            if not isinstance(issue, dict) or (issue.get("project") or {}).get("id") != project["id"]:
                raise ImportError("receipt issue %s cannot be reconciled with the selected project" % key)
            raise ImportError("receipt issue %s is missing its importer marker; refusing duplicate recreation" % key)
        if existing.get(key, {}).get("id") != issue_id:
            raise ImportError("receipt issue %s is missing or has a changed importer marker; refusing duplicate recreation" % key)
    known = dict(receipt["issues"])
    known.update(existing)
    keys = {issue["key"] for issue in backlog["issues"]}
    for key in list(known):
        if key not in keys:
            del known[key]
    for key, value in known.items():
        receipt["issues"][key] = value
    atomic_write(receipt_path, receipt)
    for issue in backlog["ordered"]:
        if issue["key"] in known:
            continue
        input_value = {"teamId": team["id"], "projectId": project["id"], "priority": issue["priority"], "title": issue["title"], "description": description_for(issue, backlog["initiative"])}
        if issue["parent"]:
            input_value["parentId"] = known[issue["parent"]]["id"]
        result = mutation(client, CREATE_ISSUE, {"input": input_value}, "issue create").get("issueCreate")
        if not isinstance(result, dict) or not result.get("success") or not isinstance(result.get("issue"), dict):
            raise UncertainMutationError("issue create outcome is uncertain; rerun to reconcile before continuing")
        created = result["issue"]
        if not created.get("id"):
            raise UncertainMutationError("issue create returned no id; rerun to reconcile before continuing")
        known[issue["key"]] = {"id": created["id"], "url": created.get("url")}
        receipt["issues"][issue["key"]] = known[issue["key"]]
        atomic_write(receipt_path, receipt)
    for issue in backlog["ordered"]:
        for dependency in issue["depends_on"]:
            relation_key = dependency + "->" + issue["key"]
            pair = (known[dependency]["id"], known[issue["key"]]["id"])
            if relation_key in receipt["dependencies"] or pair in existing_relations:
                if pair in existing_relations and relation_key not in receipt["dependencies"]:
                    receipt["dependencies"][relation_key] = {"id": None, "from": dependency, "to": issue["key"]}
                    atomic_write(receipt_path, receipt)
                continue
            result = mutation(client, CREATE_RELATION, {"input": {"issueId": pair[0], "relatedIssueId": pair[1], "type": "blocks"}}, "dependency create").get("issueRelationCreate")
            if not isinstance(result, dict) or not result.get("success"):
                raise UncertainMutationError("dependency create outcome is uncertain; rerun to reconcile before continuing")
            receipt["dependencies"][relation_key] = {"id": result.get("issueRelation", {}).get("id"), "from": dependency, "to": issue["key"]}
            atomic_write(receipt_path, receipt)
    return {"project": project, "team": team, "receipt": receipt}


def preview(backlog):
    dependencies = sum(len(issue["depends_on"]) for issue in backlog["issues"])
    parents = sum(issue["parent"] is not None for issue in backlog["issues"])
    return "validated %d tasks, %d parent links, %d dependency links\norder: %s" % (len(backlog["issues"]), parents, dependencies, ", ".join(issue["key"] for issue in backlog["ordered"]))


def main(argv=None):
    parser = argparse.ArgumentParser(description="Import atlas backlog JSON into Linear")
    parser.add_argument("backlog", type=Path)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--project-id")
    parser.add_argument("--team-id")
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args(argv)
    try:
        backlog = validate_backlog(json.loads(args.backlog.read_text(encoding="utf-8")))
        if not args.apply:
            print(preview(backlog))
            return 0
        api_key = os.environ.get("LINEAR_API_KEY")
        if not api_key:
            raise ImportError("--apply requires LINEAR_API_KEY")
        receipt = args.receipt or args.backlog.with_suffix(".linear-receipt.json")
        result = apply(backlog, LinearClient(api_key), args.project_id, args.team_id, receipt)
        print("imported %d tasks to project %s with receipt %s" % (len(result["receipt"]["issues"]), result["project"]["id"], receipt))
        return 0
    except (OSError, json.JSONDecodeError, ImportError) as error:
        print("error: %s" % error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
