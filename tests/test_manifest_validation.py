from pathlib import Path

from prro_gateway.validators.validate_manifest import validate_manifest

ROOT = Path(__file__).resolve().parents[1]


def test_manifest_validation():
    validate_manifest(ROOT)
