#!/usr/bin/env python3
"""Minimal MCP stdio server for barkeep tests: one tool, `echo`."""
import json
import sys


def tools():
    return {
        "tools": [
            {
                "name": "echo",
                "description": "Echo back the given text",
                "inputSchema": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"],
                },
            }
        ]
    }


def call_tool(args):
    text = args.get("text", "")
    return {"content": [{"type": "text", "text": f"echo: {text}"}], "isError": False}


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method", "")
        msg_id = msg.get("id")

        if method == "initialize":
            reply(msg_id, {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "echo-fixture", "version": "0.1.0"},
            })
        elif method == "notifications/initialized":
            pass
        elif method == "tools/list":
            reply(msg_id, tools())
        elif method == "tools/call":
            params = msg.get("params", {})
            name = params.get("name")
            if name != "echo":
                error(msg_id, -32602, f"Unknown tool: {name}")
            else:
                reply(msg_id, call_tool(params.get("arguments", {})))
        elif msg_id is not None:
            error(msg_id, -32601, f"Method not found: {method}")


def reply(msg_id, result):
    send({"jsonrpc": "2.0", "id": msg_id, "result": result})


def error(msg_id, code, message):
    send({"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}})


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
