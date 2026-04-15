from __future__ import annotations

import argparse
import json
from pathlib import Path

from jsonschema import Draft202012Validator
from referencing import Registry, Resource


def load_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def build_registry(root: Path) -> Registry:
    registry = Registry()
    for schema_path in sorted((root / "schemas").rglob("*.json")):
        schema = load_json(schema_path)
        schema_id = schema.get("$id")
        if schema_id:
            registry = registry.with_resource(schema_id, Resource.from_contents(schema))
    return registry


def validate_manifest(root: Path) -> None:
    manifest = load_json(root / "golden" / "manifest.json")
    registry = build_registry(root)
    for item in manifest.get("examples", []):
        schema = load_json(root / item["schema"])
        instance = load_json(root / item["path"])
        Draft202012Validator(schema, registry=registry).validate(instance)
    for item in manifest.get("golden", []):
        schema = load_json(root / item["canonical_schema"])
        expected = load_json(root / item["expected_canonical"])
        Draft202012Validator(schema, registry=registry).validate(expected)


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate schemas/examples/golden files")
    parser.add_argument("root", nargs="?", default=".")
    args = parser.parse_args()
    validate_manifest(Path(args.root).resolve())
    print("Validation OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
