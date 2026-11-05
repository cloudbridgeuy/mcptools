import json
import os
import sys
import subprocess

descendant = subprocess.Popen(["sleep", "3600"])

with open(os.environ["FIXTURE_LOG"], "a") as log:
    log.write(json.dumps({"pid": os.getpid(), "descendant": descendant.pid}) + "\n")

for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    with open(os.environ["FIXTURE_LOG"], "a") as log:
        log.write(json.dumps({"method": method, "params": request.get("params")}) + "\n")
    if "id" not in request:
        continue
    if method == "initialize":
        result = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "fake", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": os.environ.get("FIXTURE_TOOL", "echo"), "description": "Echo fixture arguments", "inputSchema": {"type": "object", "properties": {"value": {"type": "string"}}}, "annotations": {"readOnlyHint": True}}]}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": "opaque"}], "structuredContent": {"name": request["params"]["name"], "arguments": request["params"]["arguments"]}, "_meta": {"untouched": True}}
    else:
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
