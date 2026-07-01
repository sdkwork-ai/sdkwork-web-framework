from __future__ import annotations

import argparse
from pathlib import Path

from schema_registry.composer import SchemaRegistryComposer, compose_frontend_field_contract


def main() -> int:
    parser = argparse.ArgumentParser(description="Compose SDKWork schema registries")
    parser.add_argument("--app-root", type=Path, default=Path.cwd())
    subparsers = parser.add_subparsers(dest="command", required=True)

    compose = subparsers.add_parser("compose")
    compose_sub = compose.add_subparsers(dest="target", required=True)

    compose_tables = compose_sub.add_parser("tables")
    compose_tables.add_argument("--registry", type=Path, required=True)
    compose_tables.add_argument("--output", type=Path, required=True)
    compose_tables.add_argument("--require-dependencies", action="store_true")

    compose_frontend = compose_sub.add_parser("frontend-contracts")
    compose_frontend.add_argument("--index", type=Path, required=True)
    compose_frontend.add_argument("--output", type=Path, required=True)

    check = subparsers.add_parser("check")
    check_sub = check.add_subparsers(dest="target", required=True)

    check_tables = check_sub.add_parser("tables")
    check_tables.add_argument("--registry", type=Path, required=True)
    check_tables.add_argument("--output", type=Path, required=True)
    check_tables.add_argument("--require-dependencies", action="store_true")

    args = parser.parse_args()
    app_root = args.app_root.resolve()

    if args.command == "compose" and args.target == "tables":
        rendered = SchemaRegistryComposer(
            app_root,
            args.registry,
            require_dependency_registries=args.require_dependencies,
        ).render_yaml()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Wrote effective table registry to {args.output}")
        return 0

    if args.command == "compose" and args.target == "frontend-contracts":
        payload = compose_frontend_field_contract(app_root, args.index)
        import yaml

        rendered = yaml.safe_dump(payload, allow_unicode=True, sort_keys=False)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Wrote frontend field contract snapshot to {args.output}")
        return 0

    if args.command == "check" and args.target == "tables":
        rendered = SchemaRegistryComposer(
            app_root,
            args.registry,
            require_dependency_registries=args.require_dependencies,
        ).render_yaml()
        actual = args.output.read_text(encoding="utf-8")
        if actual != rendered:
            print(f"generated snapshot is stale: {args.output}")
            return 1
        print("Effective table registry snapshot is current")
        return 0

    raise SystemExit(f"unsupported command: {args.command} {args.target}")


if __name__ == "__main__":
    raise SystemExit(main())
