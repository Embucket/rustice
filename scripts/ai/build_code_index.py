#!/usr/bin/env python3
"""Generate docs/ai/code-index.md from the Rust workspace.

Run from the repository root. Requires Python 3.11 or newer for tomllib.
"""

from __future__ import annotations

import argparse
import difflib
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on interpreter version
    sys.stderr.write(
        "error: Python 3.11+ is required because this script uses stdlib tomllib\n"
    )
    raise SystemExit(2)


ROOT = Path.cwd()
OUTPUT_PATH = Path("docs/ai/code-index.md")
SCRIPT_PATH = Path("scripts/ai/build_code_index.py")
DEPENDENCY_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")


@dataclass(frozen=True)
class CrateInfo:
    name: str
    rel_path: Path
    manifest: dict[str, Any]
    kind: str
    local_dependencies: tuple[str, ...]
    features: tuple[str, ...]
    entry_points: tuple[str, ...]
    tests: tuple[str, ...]


def read_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def repo_root_check() -> None:
    if not (ROOT / "Cargo.toml").is_file() or not (ROOT / "crates").is_dir():
        raise SystemExit("error: run this script from the repository root")


def expand_workspace_members(root_manifest: dict[str, Any]) -> tuple[list[Path], list[str]]:
    workspace = root_manifest.get("workspace", {})
    member_specs = workspace.get("members", [])
    members: list[Path] = []
    missing: list[str] = []
    for spec in member_specs:
        if any(ch in spec for ch in "*?["):
            matches = sorted(path for path in ROOT.glob(spec) if path.is_dir())
            if not matches:
                missing.append(spec)
            members.extend(path.relative_to(ROOT) for path in matches)
            continue

        rel_path = Path(spec)
        if (ROOT / rel_path / "Cargo.toml").is_file():
            members.append(rel_path)
        else:
            missing.append(spec)
    return members, missing


def crate_name(manifest: dict[str, Any]) -> str:
    return manifest.get("package", {}).get("name", "<unknown>")


def manifest_maps(member_paths: list[Path]) -> tuple[dict[str, dict[str, Any]], dict[Path, str]]:
    manifests: dict[str, dict[str, Any]] = {}
    path_to_name: dict[Path, str] = {}
    for rel_path in member_paths:
        manifest = read_toml(ROOT / rel_path / "Cargo.toml")
        name = crate_name(manifest)
        manifests[name] = manifest
        path_to_name[(ROOT / rel_path).resolve()] = name
    return manifests, path_to_name


def iter_dependency_specs(manifest: dict[str, Any]):
    for section in DEPENDENCY_SECTIONS:
        for dep_name, dep_spec in manifest.get(section, {}).items():
            yield dep_name, dep_spec

    for target in manifest.get("target", {}).values():
        if not isinstance(target, dict):
            continue
        for section in DEPENDENCY_SECTIONS:
            for dep_name, dep_spec in target.get(section, {}).items():
                yield dep_name, dep_spec


def resolve_local_dependency(
    dep_name: str,
    dep_spec: Any,
    crate_dir: Path,
    local_names: set[str],
    path_to_name: dict[Path, str],
    workspace_deps: dict[str, Any],
) -> str | None:
    candidate_name = dep_name

    if isinstance(dep_spec, dict):
        candidate_name = dep_spec.get("package", dep_name)

        if "path" in dep_spec:
            dep_path = (crate_dir / dep_spec["path"]).resolve()
            return path_to_name.get(dep_path, candidate_name if candidate_name in local_names else None)

        if dep_spec.get("workspace") is True:
            workspace_spec = workspace_deps.get(dep_name)
            if isinstance(workspace_spec, dict):
                candidate_name = workspace_spec.get("package", candidate_name)
                if "path" in workspace_spec:
                    dep_path = (ROOT / workspace_spec["path"]).resolve()
                    return path_to_name.get(
                        dep_path, candidate_name if candidate_name in local_names else None
                    )

    if candidate_name in local_names:
        return candidate_name
    if dep_name in local_names:
        return dep_name
    return None


def infer_kind(manifest: dict[str, Any], crate_dir: Path) -> str:
    package = manifest.get("package", {})
    has_lib = "lib" in manifest or (crate_dir / "src/lib.rs").is_file()
    has_bin = bool(manifest.get("bin")) or (crate_dir / "src/main.rs").is_file()
    has_test_target = bool(manifest.get("test"))

    lib_table = manifest.get("lib", {})
    if has_test_target and package.get("publish") is False:
        return "test-only harness"
    if isinstance(lib_table, dict) and lib_table.get("proc-macro") is True:
        return "proc-macro lib"

    parts: list[str] = []
    if has_bin:
        parts.append("bin")
    if has_lib:
        parts.append("lib")
    if has_test_target:
        parts.append("test target")
    return " + ".join(parts) if parts else "package"


def entry_points(manifest: dict[str, Any], crate_dir: Path) -> tuple[str, ...]:
    entries: list[str] = []

    lib_table = manifest.get("lib", {})
    if isinstance(lib_table, dict) and "path" in lib_table:
        entries.append(lib_table["path"])
    elif (crate_dir / "src/lib.rs").is_file():
        entries.append("src/lib.rs")

    if (crate_dir / "src/main.rs").is_file():
        entries.append("src/main.rs")

    for bin_target in manifest.get("bin", []):
        if isinstance(bin_target, dict) and "path" in bin_target:
            entries.append(bin_target["path"])

    if (crate_dir / "build.rs").is_file():
        entries.append("build.rs")

    for test_target in manifest.get("test", []):
        if isinstance(test_target, dict) and "path" in test_target:
            entries.append(test_target["path"])

    return tuple(dict.fromkeys(entries))


def test_markers(manifest: dict[str, Any], crate_dir: Path) -> tuple[str, ...]:
    markers: list[str] = []
    if (crate_dir / "tests").is_dir():
        markers.append("tests/")
    if (crate_dir / "src/tests").is_dir():
        markers.append("src/tests/")
    if (crate_dir / "src/tests.rs").is_file():
        markers.append("src/tests.rs")
    for test_target in manifest.get("test", []):
        if isinstance(test_target, dict):
            name = test_target.get("name", "unnamed")
            markers.append(f"[[test]] {name}")
    return tuple(dict.fromkeys(markers))


def collect_crates(
    member_paths: list[Path],
    manifests: dict[str, dict[str, Any]],
    path_to_name: dict[Path, str],
    root_manifest: dict[str, Any],
) -> list[CrateInfo]:
    local_names = set(manifests)
    workspace_deps = root_manifest.get("workspace", {}).get("dependencies", {})
    crates: list[CrateInfo] = []

    for rel_path in member_paths:
        crate_dir = ROOT / rel_path
        manifest = read_toml(crate_dir / "Cargo.toml")
        name = crate_name(manifest)
        local_dependencies = sorted(
            dep
            for dep_name, dep_spec in iter_dependency_specs(manifest)
            if (
                dep := resolve_local_dependency(
                    dep_name,
                    dep_spec,
                    crate_dir,
                    local_names,
                    path_to_name,
                    workspace_deps,
                )
            )
            is not None
            and dep != name
        )
        features = tuple(sorted(manifest.get("features", {}).keys()))
        crates.append(
            CrateInfo(
                name=name,
                rel_path=rel_path,
                manifest=manifest,
                kind=infer_kind(manifest, crate_dir),
                local_dependencies=tuple(dict.fromkeys(local_dependencies)),
                features=features,
                entry_points=entry_points(manifest, crate_dir),
                tests=test_markers(manifest, crate_dir),
            )
        )

    return sorted(crates, key=lambda crate: crate.name)


def crates_dir_manifests_not_in_workspace(member_paths: list[Path]) -> list[str]:
    workspace_paths = {path.as_posix() for path in member_paths}
    paths = []
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        rel = manifest.parent.relative_to(ROOT).as_posix()
        if rel not in workspace_paths:
            paths.append(rel)
    return paths


def markdown_cell(values: tuple[str, ...] | list[str]) -> str:
    if not values:
        return "none"
    return ", ".join(f"`{value}`" for value in values).replace("|", "\\|")


def command_rows(crates: list[CrateInfo]) -> list[tuple[str, str, str]]:
    crate_names = {crate.name for crate in crates}
    rows: list[tuple[str, str, str]] = []

    makefile = ROOT / "Makefile"
    if makefile.is_file():
        makefile_text = makefile.read_text(encoding="utf-8")
        if "$(CARGO) build" in makefile_text:
            rows.append(("`make build`", "Makefile", "debug build (`cargo build`)"))
        if "$(CARGO) build --release" in makefile_text:
            rows.append(("`make release`", "Makefile", "release build (`cargo build --release`)"))
        if "$(CARGO) test" in makefile_text:
            rows.append(("`make test`", "Makefile", "workspace default test loop (`cargo test`)"))

    if "functions" in crate_names:
        rows.append(("`cargo test -p functions`", "Cargo workspace", "function crate tests"))
    if "executor" in crate_names:
        rows.append(("`cargo test -p executor`", "Cargo workspace", "executor and planner tests"))
    if "api-snowflake-rest" in crate_names:
        rows.append(
            (
                "`cargo test -p api-snowflake-rest -- --test-threads=1`",
                "Cargo workspace",
                "Snowflake-compatible REST API tests; serialized because local server tests share state",
            )
        )
    if "api-snowflake-rest-sessions" in crate_names:
        rows.append(
            (
                "`cargo test -p api-snowflake-rest-sessions`",
                "Cargo workspace",
                "session/auth extraction tests",
            )
        )
    if "embucket-sqllogictest" in crate_names:
        rows.append(
            (
                "`cargo test -p embucket-sqllogictest`",
                "Cargo workspace",
                "offline SQL logic compatibility harness",
            )
        )

    tests_workflow = ROOT / ".github/workflows/tests.yml"
    if tests_workflow.is_file():
        rows.append(
            (
                "`cargo test --profile=ci --workspace --all-targets --exclude api-snowflake-rest --exclude embucket-sqllogictest`",
                ".github/workflows/tests.yml",
                "main CI workspace test command",
            )
        )
        rows.append(
            (
                "`cargo test --profile=ci -p api-snowflake-rest --all-targets -- --test-threads=1`",
                ".github/workflows/tests.yml",
                "main CI REST API command",
            )
        )
        if "embucket-sqllogictest" in crate_names:
            rows.append(
                (
                    "`cargo test -p embucket-sqllogictest --profile=ci --test sqllogictests -- --test-threads $(nproc)`",
                    ".github/workflows/tests.yml",
                    "non-gating CI SQL logic command",
                )
            )

    check_workflow = ROOT / ".github/workflows/check.yml"
    if check_workflow.is_file():
        rows.append(("`cargo fmt --check`", ".github/workflows/check.yml", "format check"))
        rows.append(
            (
                "`cargo clippy --all-targets --workspace`",
                ".github/workflows/check.yml",
                "workspace lint check",
            )
        )

    rows.append(("`python3 scripts/ai/build_code_index.py --check`", "this generator", "verify generated index freshness"))
    return rows


def path_exists(path: str) -> bool:
    return (ROOT / path).exists()


def render_code_index(
    root_manifest: dict[str, Any],
    member_paths: list[Path],
    missing_members: list[str],
    crates: list[CrateInfo],
    non_workspace_crates: list[str],
) -> str:
    default_members = root_manifest.get("workspace", {}).get("default-members", [])
    lines: list[str] = [
        "# Rustice code index",
        "",
        f"Generated by `{SCRIPT_PATH.as_posix()}`. Do not edit by hand.",
        "",
        "## Workspace summary",
        "",
        f"- Workspace members declared in `Cargo.toml`: {len(member_paths)}",
        f"- Default members: {markdown_cell(tuple(default_members))}",
        f"- Resolver: `{root_manifest.get('workspace', {}).get('resolver', 'unknown')}`",
    ]
    if missing_members:
        lines.append(f"- Missing workspace member paths: {markdown_cell(tuple(sorted(missing_members)))}")
    if non_workspace_crates:
        lines.append(
            f"- Crate manifests under `crates/` but outside the declared workspace: {markdown_cell(tuple(non_workspace_crates))}"
        )
    lines.extend(
        [
            "",
            "## Crates",
            "",
            "| Crate | Path | Kind | Local dependencies | Features | Entry points | Tests |",
            "|---|---|---|---|---|---|---|",
        ]
    )
    for crate in crates:
        lines.append(
            "| "
            + " | ".join(
                [
                    f"`{crate.name}`",
                    f"`{crate.rel_path.as_posix()}`",
                    crate.kind,
                    markdown_cell(crate.local_dependencies),
                    markdown_cell(crate.features),
                    markdown_cell(crate.entry_points),
                    markdown_cell(crate.tests),
                ]
            )
            + " |"
        )

    lines.extend(["", "## Local dependency graph", ""])
    for crate in crates:
        deps = ", ".join(f"`{dep}`" for dep in crate.local_dependencies) or "none"
        lines.append(f"- `{crate.name}` -> {deps}")

    lines.extend(["", "## Important execution paths", ""])
    if path_exists("crates/api-snowflake-rest/src/server/logic.rs") and path_exists(
        "crates/executor/src/service.rs"
    ):
        lines.extend(
            [
                "### API request to query execution",
                "",
                "- `api-snowflake-rest/src/server/router.rs` defines the Snowflake-compatible login/query routes.",
                "- `api-snowflake-rest/src/server/handlers.rs` accepts Axum requests and calls `handle_query_request`.",
                "- `api-snowflake-rest/src/server/logic.rs` builds `executor::models::QueryContext` and calls `ExecutionService::query`.",
                "- `executor/src/service.rs` owns session/query lifecycle; `executor/src/session.rs` builds the DataFusion `SessionContext` with catalog, planner, optimizer, and function registration.",
                "",
            ]
        )

    if path_exists("crates/functions/src/lib.rs") and path_exists("crates/executor/src/session.rs"):
        lines.extend(
            [
                "### Function compatibility",
                "",
                "- `functions/src/lib.rs` exposes `register_udfs` and re-exports aggregate registration.",
                "- `functions/src/table/mod.rs` registers table functions where present.",
                "- `executor/src/session.rs` calls function registration while constructing each DataFusion session.",
                "",
            ]
        )

    if path_exists("crates/sqllogictest/tests/sqllogictests.rs"):
        lines.extend(
            [
                "### SQL logic tests",
                "",
                "- `sqllogictest/tests/sqllogictests.rs` discovers `.slt` files under `crates/sqllogictest/tests/slt`.",
                "- The harness creates local executor sessions through `executor::test_helpers::create_df_session_with_catalog_url(\"/dev\")`.",
                "- This path is local/offline; it does not require Snowflake for the default development loop.",
                "",
            ]
        )

    lines.extend(
        [
            "## Verification commands",
            "",
            "| Command | Source | Use |",
            "|---|---|---|",
        ]
    )
    for command, source, use in command_rows(crates):
        lines.append(f"| {command} | `{source}` | {use} |")

    lines.extend(["", "## Notes for agents", ""])
    lines.extend(
        [
            f"- Regenerate this file with `python3 {SCRIPT_PATH.as_posix()}`.",
            f"- Check freshness with `python3 {SCRIPT_PATH.as_posix()} --check`.",
            "- Use crate-local `AGENTS.md` files for ownership and boundary notes before editing a crate.",
            "- Use `docs/testing/test-matrix.md` for change-specific local verification choices.",
            "- Prefer crate-level `cargo test -p ...` commands before wider workspace loops.",
        ]
    )
    if path_exists("Makefile") and not path_exists("tests"):
        lines.append(
            "- `make integration-test` exists in the root Makefile, but this checkout has no root `tests/` directory; verify before relying on that target."
        )
    if non_workspace_crates:
        lines.append(
            "- Do not treat non-workspace crate manifests as build participants unless they are added to the root workspace."
        )

    lines.append("")
    return "\n".join(lines)


def build_content() -> str:
    repo_root_check()
    root_manifest = read_toml(ROOT / "Cargo.toml")
    member_paths, missing_members = expand_workspace_members(root_manifest)
    manifests, path_to_name = manifest_maps(member_paths)
    crates = collect_crates(member_paths, manifests, path_to_name, root_manifest)
    non_workspace_crates = crates_dir_manifests_not_in_workspace(member_paths)
    return render_code_index(
        root_manifest, member_paths, missing_members, crates, non_workspace_crates
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if docs/ai/code-index.md is missing or stale",
    )
    args = parser.parse_args()

    content = build_content()
    output_path = ROOT / OUTPUT_PATH

    if args.check:
        if not output_path.is_file():
            sys.stderr.write(f"error: {OUTPUT_PATH.as_posix()} does not exist\n")
            return 1
        existing = output_path.read_text(encoding="utf-8")
        if existing != content:
            diff = difflib.unified_diff(
                existing.splitlines(),
                content.splitlines(),
                fromfile=OUTPUT_PATH.as_posix(),
                tofile=f"{OUTPUT_PATH.as_posix()} (generated)",
                lineterm="",
            )
            sys.stderr.write("error: generated code index is stale\n")
            sys.stderr.write("\n".join(diff))
            sys.stderr.write("\n")
            return 1
        print(f"{OUTPUT_PATH.as_posix()} is up to date")
        return 0

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(content, encoding="utf-8")
    print(f"wrote {OUTPUT_PATH.as_posix()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
