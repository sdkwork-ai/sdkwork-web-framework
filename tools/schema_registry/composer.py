from __future__ import annotations

import json
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError as exc:  # pragma: no cover
    yaml = None
    _YAML_IMPORT_ERROR = exc
else:
    _YAML_IMPORT_ERROR = None


class SchemaRegistryLoadError(ValueError):
    """Raised when the schema registry or one of its fragments is malformed."""


METADATA_KEYS = {"schema", "version", "source", "rule"}
MERGEABLE_LIST_SECTIONS = {"frontend_models", "frontend_operations", "routes"}
MERGEABLE_MAPPING_SECTIONS = {"x_response_entities"}


@dataclass(frozen=True)
class RegistryDependency:
    module_id: str
    locator: Path
    registry_path: Path
    order: int
    ownership: str = "read_only"

    def resolve_registry_file(self, app_root: Path) -> Path:
        return (app_root / self.locator / self.registry_path).resolve()


class SchemaRegistryComposer:
    def __init__(
        self,
        app_root: Path,
        registry_path: Path,
        *,
        require_dependency_registries: bool = False,
    ) -> None:
        self.app_root = Path(app_root).resolve()
        self.registry_path = Path(registry_path).resolve()
        self.require_dependency_registries = require_dependency_registries

    def compose(self) -> dict[str, Any]:
        return _compose_schema_registry(
            self.app_root,
            self.registry_path,
            require_dependency_registries=self.require_dependency_registries,
        )

    def render_yaml(self) -> str:
        if yaml is None:
            raise RuntimeError("PyYAML is required to render schema registry YAML") from _YAML_IMPORT_ERROR
        return yaml.safe_dump(self.compose(), allow_unicode=True, sort_keys=False)

    def source_paths(self) -> list[Path]:
        return schema_registry_source_paths(
            self.registry_path,
            app_root=self.app_root,
            require_dependency_registries=self.require_dependency_registries,
        )


def load_schema_registry(path: Path, *, app_root: Path | None = None) -> dict[str, Any]:
    resolved_root = Path(app_root).resolve() if app_root is not None else _infer_app_root(path)
    return SchemaRegistryComposer(resolved_root, path).compose()


def render_schema_registry(path: Path, *, app_root: Path | None = None) -> str:
    resolved_root = Path(app_root).resolve() if app_root is not None else _infer_app_root(path)
    return SchemaRegistryComposer(resolved_root, path).render_yaml()


def schema_registry_source_paths(
    path: Path,
    *,
    app_root: Path | None = None,
    require_dependency_registries: bool = False,
) -> list[Path]:
    registry_path = Path(path).resolve()
    resolved_root = Path(app_root).resolve() if app_root is not None else _infer_app_root(registry_path)
    registry = _load_mapping(registry_path)
    paths = [registry_path, *_local_fragment_paths(registry_path, registry)]
    for dependency in _resolve_registry_dependencies(resolved_root, registry):
        dependency_registry = dependency.resolve_registry_file(resolved_root)
        if dependency_registry.is_file():
            dependency_payload = _load_mapping(dependency_registry)
            paths.append(dependency_registry)
            paths.extend(_local_fragment_paths(dependency_registry, dependency_payload))
        elif require_dependency_registries:
            raise SchemaRegistryLoadError(
                f"dependency registry not found: module={dependency.module_id}, path={dependency_registry}"
            )
    return paths


def compose_frontend_field_contract(root: Path, index_path: Path | None = None) -> dict[str, Any]:
    root = Path(root).resolve()
    selected_index = (
        Path(index_path).resolve()
        if index_path is not None
        else root / "docs" / "schema-registry" / "frontend-field-contracts" / "index.yaml"
    )
    index = _load_mapping(selected_index, "frontend field contract index")
    if not _is_fragment_index(index):
        return index

    compiled: dict[str, Any] = {}
    for key in METADATA_KEYS:
        if key in index:
            compiled[key] = index[key]

    fragments = index.get("fragments")
    if not isinstance(fragments, list) or not fragments:
        raise SchemaRegistryLoadError("frontend field contract index fragments must be a non-empty list")

    seen_fragments: set[Path] = set()
    for raw_fragment in fragments:
        fragment_path = _frontend_fragment_path(selected_index, raw_fragment)
        if fragment_path in seen_fragments:
            raise SchemaRegistryLoadError(f"duplicate frontend field contract fragment: {_display_path(fragment_path)}")
        seen_fragments.add(fragment_path)
        fragment = _load_mapping(fragment_path, f"frontend field contract fragment {_display_path(fragment_path)}")
        _merge_frontend_fragment(compiled, fragment, fragment_path=fragment_path)

    for section in MERGEABLE_LIST_SECTIONS:
        compiled.setdefault(section, [])
    for section in MERGEABLE_MAPPING_SECTIONS:
        compiled.setdefault(section, {})
    return compiled


def _compose_schema_registry(
    app_root: Path,
    registry_path: Path,
    *,
    require_dependency_registries: bool,
) -> dict[str, Any]:
    registry = _load_mapping(registry_path)
    merged = deepcopy(registry)

    dependency_tables = _load_dependency_tables(
        app_root,
        registry_path,
        registry,
        require_dependency_registries=require_dependency_registries,
    )
    assembly_tables = _collect_local_tables(registry_path, registry)
    _validate_assembly_tables(dependency_tables, assembly_tables)

    effective_tables: dict[str, dict[str, Any]] = dict(dependency_tables)
    for table in assembly_tables:
        table_name = _table_name(table)
        if table_name in effective_tables:
            if not _table_matches(effective_tables[table_name], table):
                raise SchemaRegistryLoadError(
                    f"duplicate table `{table_name}` from assembly registry conflicts with dependency table definition"
                )
            continue
        effective_tables[table_name] = _annotate_table(table, imported=False)

    tables = [effective_tables[name] for name in sorted(effective_tables)]
    merged["tables"] = tables
    merged.pop("table_fragments", None)
    merged.pop("registry_dependencies", None)
    return merged


def _load_dependency_tables(
    app_root: Path,
    registry_path: Path,
    registry: dict[str, Any],
    *,
    require_dependency_registries: bool,
) -> dict[str, dict[str, Any]]:
    tables: dict[str, dict[str, Any]] = {}
    for dependency in _resolve_registry_dependencies(app_root, registry):
        dependency_registry = dependency.resolve_registry_file(app_root)
        if not dependency_registry.is_file():
            if require_dependency_registries:
                raise SchemaRegistryLoadError(
                    f"dependency registry not found: module={dependency.module_id}, path={dependency_registry}"
                )
            continue
        dependency_payload = _load_mapping(dependency_registry)
        for table in _collect_local_tables(dependency_registry, dependency_payload):
            table_name = _table_name(table)
            if table_name in tables:
                raise SchemaRegistryLoadError(
                    f"duplicate table `{table_name}` from dependency modules declare the same table"
                )
            tables[table_name] = _annotate_table(table, imported=True, dependency_module=dependency.module_id)
    return tables


def _validate_assembly_tables(
    dependency_tables: dict[str, dict[str, Any]],
    assembly_tables: list[dict[str, Any]],
) -> None:
    for table in assembly_tables:
        table_name = _table_name(table)
        if table_name not in dependency_tables:
            continue
        if table.get("imported"):
            continue
        if table.get("generated_by_this_project") is False:
            continue
        if table.get("system_of_record") is False:
            continue
        source_refs = table.get("source_refs")
        if isinstance(source_refs, list) and source_refs:
            continue
        raise SchemaRegistryLoadError(
            f"table `{table_name}` conflicts with dependency-owned table `{table_name}` without source_refs"
        )


def _collect_local_tables(registry_path: Path, registry: dict[str, Any]) -> list[dict[str, Any]]:
    tables = list(_inline_tables(registry))
    for fragment_path in _local_fragment_paths(registry_path, registry):
        fragment = _load_mapping(fragment_path)
        tables.extend(_inline_tables(fragment))
    return tables


def _inline_tables(registry: dict[str, Any]) -> list[dict[str, Any]]:
    inline_tables = registry.get("tables", [])
    if inline_tables is None:
        return []
    if not isinstance(inline_tables, list):
        raise SchemaRegistryLoadError("tables must be a list")
    return [table for table in inline_tables if isinstance(table, dict)]


def _local_fragment_paths(registry_path: Path, registry: dict[str, Any]) -> list[Path]:
    fragments = _string_list(registry.get("table_fragments"))
    registry_dir = registry_path.parent.resolve()
    paths: list[Path] = []
    for fragment in fragments:
        fragment_path = (registry_dir / fragment).resolve()
        try:
            fragment_path.relative_to(registry_dir)
        except ValueError as exc:
            raise SchemaRegistryLoadError(
                f"table fragment must stay under schema registry directory: {fragment}"
            ) from exc
        if not fragment_path.exists():
            raise FileNotFoundError(f"schema registry table fragment not found: {fragment_path}")
        paths.append(fragment_path)
    return paths


def _resolve_registry_dependencies(app_root: Path, registry: dict[str, Any]) -> list[RegistryDependency]:
    explicit = registry.get("registry_dependencies")
    if isinstance(explicit, list) and explicit:
        return _parse_registry_dependencies(explicit)
    return _derive_registry_dependencies_from_manifest(app_root)


def _parse_registry_dependencies(items: list[Any]) -> list[RegistryDependency]:
    dependencies: list[RegistryDependency] = []
    for item in items:
        if not isinstance(item, dict):
            raise SchemaRegistryLoadError("registry_dependencies entries must be mappings")
        module_id = item.get("module_id")
        locator = item.get("locator")
        if not isinstance(module_id, str) or not module_id:
            raise SchemaRegistryLoadError("registry_dependencies.module_id is required")
        if not isinstance(locator, str) or not locator:
            raise SchemaRegistryLoadError("registry_dependencies.locator is required")
        registry_path = item.get("registry_path")
        if registry_path is None:
            registry_path = f"docs/schema-registry/{module_id}.tables.yaml"
        if not isinstance(registry_path, str) or not registry_path:
            raise SchemaRegistryLoadError("registry_dependencies.registry_path must be a non-empty string")
        order = item.get("order", 0)
        if not isinstance(order, int):
            raise SchemaRegistryLoadError("registry_dependencies.order must be an integer")
        ownership = item.get("ownership", "read_only")
        if not isinstance(ownership, str):
            raise SchemaRegistryLoadError("registry_dependencies.ownership must be a string")
        dependencies.append(
            RegistryDependency(
                module_id=module_id,
                locator=Path(locator),
                registry_path=Path(registry_path),
                order=order,
                ownership=ownership,
            )
        )
    dependencies.sort(key=lambda item: (item.order, item.module_id))
    return dependencies


def _derive_registry_dependencies_from_manifest(app_root: Path) -> list[RegistryDependency]:
    manifest_path = app_root / "database" / "database.manifest.json"
    if not manifest_path.is_file():
        return []
    payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    modules = payload.get("modules", [])
    if not isinstance(modules, list):
        raise SchemaRegistryLoadError("database.manifest.json modules must be a list")
    dependencies: list[RegistryDependency] = []
    for module in modules:
        if not isinstance(module, dict):
            raise SchemaRegistryLoadError("database.manifest.json module entries must be mappings")
        module_id = module.get("moduleId")
        locator = module.get("locator")
        if not isinstance(module_id, str) or not module_id:
            raise SchemaRegistryLoadError("database.manifest.json moduleId is required")
        if not isinstance(locator, str) or not locator:
            raise SchemaRegistryLoadError("database.manifest.json locator is required")
        order = module.get("order", 0)
        if not isinstance(order, int):
            raise SchemaRegistryLoadError("database.manifest.json order must be an integer")
        dependencies.append(
            RegistryDependency(
                module_id=module_id,
                locator=Path(locator),
                registry_path=Path(f"docs/schema-registry/{module_id}.tables.yaml"),
                order=order,
            )
        )
    dependencies.sort(key=lambda item: (item.order, item.module_id))
    return dependencies


def _table_name(table: dict[str, Any]) -> str:
    table_name = table.get("table")
    if not isinstance(table_name, str) or not table_name:
        raise SchemaRegistryLoadError("each table entry must declare table")
    return table_name


def _annotate_table(
    table: dict[str, Any],
    *,
    imported: bool,
    dependency_module: str | None = None,
) -> dict[str, Any]:
    annotated = deepcopy(table)
    annotated["imported"] = imported
    if imported:
        annotated["generated_by_this_project"] = False
        if dependency_module:
            annotated["imported_from_module"] = dependency_module
    return annotated


def _table_matches(left: dict[str, Any], right: dict[str, Any]) -> bool:
    ignored = {"imported", "imported_from_module"}
    left_keys = {key for key in left if key not in ignored}
    right_keys = {key for key in right if key not in ignored}
    return left_keys == right_keys


def _is_fragment_index(payload: dict[str, Any]) -> bool:
    return isinstance(payload.get("fragments"), list)


def _frontend_fragment_path(index_path: Path, raw_fragment: Any) -> Path:
    if isinstance(raw_fragment, str):
        raw_path = raw_fragment
    elif isinstance(raw_fragment, dict) and isinstance(raw_fragment.get("path"), str):
        raw_path = raw_fragment["path"]
    else:
        raise SchemaRegistryLoadError(
            "frontend field contract fragment entries must be strings or mappings with path"
        )
    candidate = Path(raw_path)
    if candidate.is_absolute() or ".." in candidate.parts:
        raise SchemaRegistryLoadError(
            f"frontend field contract fragment path must stay inside the contract directory: {raw_path}"
        )
    return (index_path.parent / candidate).resolve()


def _merge_frontend_fragment(
    compiled: dict[str, Any],
    fragment: dict[str, Any],
    *,
    fragment_path: Path,
) -> None:
    for key, value in fragment.items():
        if key in METADATA_KEYS or key == "fragment":
            continue
        if key in MERGEABLE_LIST_SECTIONS:
            if not isinstance(value, list):
                raise SchemaRegistryLoadError(f"{_display_path(fragment_path)} {key} must be a list")
            compiled.setdefault(key, [])
            if not isinstance(compiled[key], list):
                raise SchemaRegistryLoadError(f"frontend field contract section {key} cannot be both list and mapping")
            compiled[key].extend(value)
            continue
        if key in MERGEABLE_MAPPING_SECTIONS:
            if not isinstance(value, dict):
                raise SchemaRegistryLoadError(f"{_display_path(fragment_path)} {key} must be a mapping")
            compiled.setdefault(key, {})
            if not isinstance(compiled[key], dict):
                raise SchemaRegistryLoadError(f"frontend field contract section {key} cannot be both mapping and list")
            duplicate_keys = set(compiled[key]) & set(value)
            if duplicate_keys:
                duplicates = ", ".join(sorted(str(item) for item in duplicate_keys))
                raise SchemaRegistryLoadError(
                    f"{_display_path(fragment_path)} declares duplicate {key}: {duplicates}"
                )
            compiled[key].update(value)
            continue
        if key in compiled:
            raise SchemaRegistryLoadError(
                f"{_display_path(fragment_path)} declares duplicate frontend field contract section: {key}"
            )
        compiled[key] = value


def _load_mapping(path: Path, label: str | None = None) -> dict[str, Any]:
    if yaml is None:
        raise RuntimeError("PyYAML is required to load schema registry YAML") from _YAML_IMPORT_ERROR
    if not path.exists():
        if label:
            return {}
        raise FileNotFoundError(f"schema registry not found: {path}")
    payload = yaml.safe_load(path.read_text(encoding="utf-8"))
    if payload is None:
        return {}
    if not isinstance(payload, dict):
        message = label or _display_path(path)
        raise SchemaRegistryLoadError(f"{message} root must be a mapping")
    return payload


def _string_list(value: Any) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list):
        raise SchemaRegistryLoadError("table_fragments must be a list")
    fragments: list[str] = []
    for item in value:
        if not isinstance(item, str) or not item:
            raise SchemaRegistryLoadError("table_fragments entries must be non-empty strings")
        fragments.append(item)
    return fragments


def _infer_app_root(registry_path: Path) -> Path:
    resolved = registry_path.resolve()
    for candidate in [resolved.parent, *resolved.parents]:
        if (candidate / "database" / "database.manifest.json").is_file():
            return candidate
        if (candidate / "sdkwork.app.config.json").is_file():
            return candidate
    return resolved.parent.parent.parent


def _display_path(path: Path) -> str:
    return path.as_posix()
