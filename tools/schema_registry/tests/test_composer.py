from __future__ import annotations

import sys
from pathlib import Path

import pytest

TOOLS_ROOT = Path(__file__).resolve().parents[1]
if str(TOOLS_ROOT) not in sys.path:
    sys.path.insert(0, str(TOOLS_ROOT))

from schema_registry.composer import (  # noqa: E402
    SchemaRegistryComposer,
    SchemaRegistryLoadError,
    compose_frontend_field_contract,
)


def test_merges_local_table_fragments(tmp_path: Path) -> None:
    registry_dir = tmp_path / "docs/schema-registry"
    (registry_dir / "tables").mkdir(parents=True)
    (registry_dir / "app.tables.yaml").write_text(
        """
tables:
  - table: ai_runtime
    domain: ai
table_fragments:
  - tables/010-ai.yaml
""".strip(),
        encoding="utf-8",
    )
    (registry_dir / "tables/010-ai.yaml").write_text(
        """
tables:
  - table: ai_channel
    domain: ai
""".strip(),
        encoding="utf-8",
    )

    composed = SchemaRegistryComposer(tmp_path, registry_dir / "app.tables.yaml").compose()
    table_names = {table["table"] for table in composed["tables"]}
    assert table_names == {"ai_runtime", "ai_channel"}


def test_rejects_conflicting_dependency_projection_without_source_refs(tmp_path: Path) -> None:
    registry_dir = tmp_path / "docs/schema-registry"
    dependency_dir = tmp_path / "dependency"
    registry_dir.mkdir(parents=True)
    (dependency_dir / "docs/schema-registry").mkdir(parents=True)
    (dependency_dir / "docs/schema-registry/commerce-core.tables.yaml").write_text(
        """
tables:
  - table: commerce_wallet
    domain: commerce
    system_of_record: true
""".strip(),
        encoding="utf-8",
    )
    (tmp_path / "database").mkdir()
    (tmp_path / "database/database.manifest.json").write_text(
        '{"modules":[{"moduleId":"commerce-core","locator":"dependency","order":10}]}',
        encoding="utf-8",
    )
    (registry_dir / "app.tables.yaml").write_text(
        """
tables:
  - table: commerce_wallet
    domain: commerce
    system_of_record: true
    generated_by_this_project: true
""".strip(),
        encoding="utf-8",
    )

    with pytest.raises(SchemaRegistryLoadError):
        SchemaRegistryComposer(tmp_path, registry_dir / "app.tables.yaml").compose()


def test_merges_frontend_contract_fragments(tmp_path: Path) -> None:
    contract_dir = tmp_path / "docs/schema-registry/frontend-field-contracts"
    (contract_dir / "models").mkdir(parents=True)
    (contract_dir / "index.yaml").write_text(
        """
schema: demo
version: 0.1.0
fragments:
  - models/a.yaml
  - models/b.yaml
""".strip(),
        encoding="utf-8",
    )
    (contract_dir / "models/a.yaml").write_text("frontend_models:\n  - id: a\n", encoding="utf-8")
    (contract_dir / "models/b.yaml").write_text("frontend_models:\n  - id: b\n", encoding="utf-8")

    composed = compose_frontend_field_contract(tmp_path, contract_dir / "index.yaml")
    assert len(composed["frontend_models"]) == 2
