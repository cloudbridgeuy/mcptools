import importlib.util
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location("baseline_record", Path(__file__).with_name("baseline_record.py"))
RECORD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RECORD)

BUILD = "writeExtractCache enrichGraph checkpoint writeGraph discoverScopes scopeOf walkDir TS/Python errors.push"
CLI = "discoverWorkspaceChildren"


class VerifyGraft(unittest.TestCase):
    def test_checklist_true_on_markers(self):
        graft = RECORD.verify_graft("abc", BUILD, CLI, ["rust", "c"])
        self.assertEqual(graft["rev"], "abc")
        self.assertTrue(graft["separation"])
        self.assertIn("walkDir", graft["discovery"])
        self.assertIn("federates queries across git children", graft["discovery"])
        self.assertIn("rust", graft["limits"][1])
        self.assertEqual(len(graft["unsupported"]), 3)

    def test_checklist_false_without_enrich_marker(self):
        graft = RECORD.verify_graft("abc", "walkDir", "", [])
        self.assertFalse(graft["separation"])
        self.assertEqual(graft["checkpointing"], "no checkpoint markers found in pinned build.ts")


class Redact(unittest.TestCase):
    def test_masks_tokens(self):
        env = {"hardware": "arm64 sk-abc123", "note": "token=xyz"}
        redacted = RECORD.redact_env(env)
        self.assertNotIn("sk-abc123", redacted["hardware"])
        self.assertIn("[redacted]", redacted["hardware"])
        self.assertIn("[redacted]", redacted["note"])

    def test_leaves_plain_values(self):
        env = {"binary_version": "mcptools 0.1.0"}
        self.assertEqual(RECORD.redact_env(env), env)


class Markdown(unittest.TestCase):
    def record(self):
        env = {
            "mcptools_rev": "aaa", "mcptools_dirty": "", "llm_stream_rev": "bbb",
            "llm_stream_dirty": "", "binary_version": "mcptools 0.1.0",
            "file_count": 89, "dir_count": 22, "hardware": "arm64",
            "cache_state": "no primer; no local db; fresh temp db per trial",
        }
        trials = {"initial": [0.2, 0.3, 0.25], "incremental": [0.05, 0.05, 0.06],
                  "tree": [0.01, 0.02, 0.01], "peek": [0.07, 0.08, 0.07]}
        counts = {"files": 89, "dirs": 22, "symbols": 527}
        old = {"initial": 0.262, "incremental": 0.051, "tree": 0.015, "peek": 0.075}
        graft = RECORD.verify_graft("abc", BUILD, CLI, ["rust"])
        return RECORD.render_record(env, trials, counts, old,
                                    RECORD.enrichment_missing("chatgpt", "x", "unsupported"),
                                    [], graft, {"date": "2026-09-11", "run_dir": "atlas-baseline-1"})

    def test_labels_old_and_avoids_timing_comparison(self):
        text = RECORD.render_markdown(self.record())
        self.assertIn("old single local run", text)
        for word in ("faster", "slower", "speedup", "speed-up", "outperforms"):
            self.assertNotIn(word, text)

    def test_json_round_trip(self):
        import json
        record = self.record()
        parsed = json.loads(json.dumps(record))
        self.assertEqual(parsed["graft"]["rev"], "abc")
        self.assertEqual(parsed["meta"]["date"], "2026-09-11")


if __name__ == "__main__":
    unittest.main()
