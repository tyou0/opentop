#!/usr/bin/env python3
import json

print(json.dumps({
    "protocol_version": 1,
    "lines": ["Hello from OpenTop", "This panel runs as your user."],
}))
