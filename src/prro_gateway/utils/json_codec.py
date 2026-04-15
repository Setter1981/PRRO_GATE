import json
from typing import Any


def dumps_json(value: Any | None) -> str | None:
    if value is None:
        return None
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def loads_json(value: str | None) -> Any | None:
    if value is None or value == "":
        return None
    return json.loads(value)
