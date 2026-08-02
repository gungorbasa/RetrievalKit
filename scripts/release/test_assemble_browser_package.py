from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("assemble_browser_package.py")
SPEC = importlib.util.spec_from_file_location("assemble_browser_package", SCRIPT)
assert SPEC and SPEC.loader
ASSEMBLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSEMBLER)
VALIDATOR_SPEC = importlib.util.spec_from_file_location(
    "validate_release", Path(__file__).with_name("validate_release.py")
)
assert VALIDATOR_SPEC and VALIDATOR_SPEC.loader
VALIDATOR = importlib.util.module_from_spec(VALIDATOR_SPEC)
VALIDATOR_SPEC.loader.exec_module(VALIDATOR)


class BrowserPackageAssemblyTests(unittest.TestCase):
    def write_package(self, root: Path) -> Path:
        package_root = root / "package"
        metadata = {
            "name": "@gungorbasa/retrievalkit-browser",
            "version": "0.1.0",
            "private": True,
            "description": "source placeholder",
            "type": "module",
            "license": "Apache-2.0",
            "repository": ASSEMBLER.EXPECTED_REPOSITORY,
            "files": ["dist", "LICENSE", "NOTICE", "README.md"],
            "exports": {
                ".": {"types": "./dist/index.d.ts", "import": "./dist/index.js"},
                "./worker": {
                    "types": "./dist/worker.d.ts",
                    "import": "./dist/worker.js",
                },
                "./adapter": {
                    "types": "./dist/adapter.d.ts",
                    "import": "./dist/adapter.js",
                },
            },
            "scripts": {"check": "true", "build": "true"},
            "devDependencies": {"typescript": "0.0.0"},
            "engines": {"node": "^22.13.0 || ^24.0.0"},
        }
        package_root.mkdir()
        (package_root / "package.json").write_text(
            json.dumps(metadata), encoding="utf-8"
        )
        repository = Path(__file__).resolve().parents[2]
        for name in ("LICENSE", "NOTICE"):
            (package_root / name).write_bytes((repository / name).read_bytes())
        (package_root / "README.md").write_text("browser", encoding="utf-8")
        dist = package_root / "dist"
        dist.mkdir()
        for module in ASSEMBLER.TYPESCRIPT_MODULES:
            for suffix in (".js", ".js.map", ".d.ts", ".d.ts.map"):
                (dist / f"{module}{suffix}").write_text(
                    "{}\n" if suffix.endswith(".map") else "export {};\n",
                    encoding="utf-8",
                )
        (dist / "index.js").write_text(
            "export class RetrievalKitBrowser {}\n", encoding="utf-8"
        )
        (dist / "index.d.ts").write_text(
            "export declare class RetrievalKitBrowser {}\n", encoding="utf-8"
        )
        (dist / "worker.js").write_text(
            "export function installRetrievalKitWorker() {}\n", encoding="utf-8"
        )
        (dist / "worker.d.ts").write_text(
            "export declare function installRetrievalKitWorker(): void;\n",
            encoding="utf-8",
        )
        (dist / "adapter.js").write_text(
            "export function createAdaptiveGeneratedWasmAdapter() {}\n",
            encoding="utf-8",
        )
        (dist / "adapter.d.ts").write_text(
            "export declare function createAdaptiveGeneratedWasmAdapter(): void;\n",
            encoding="utf-8",
        )
        return package_root

    def write_generated(self, root: Path) -> Path:
        generated = root / "generated"
        for index, tier in enumerate(ASSEMBLER.WASM_TIERS, start=1):
            target = generated / tier
            target.mkdir(parents=True)
            (target / "retrievalkit_wasm.js").write_text(
                "export default async function init() {}\n", encoding="utf-8"
            )
            (target / "retrievalkit_wasm.d.ts").write_text(
                "export default function init(): Promise<void>;\n", encoding="utf-8"
            )
            (target / "retrievalkit_wasm_bg.wasm").write_bytes(
                b"\x00asm\x01\x00\x00\x00" + bytes([index])
            )
            (target / "retrievalkit_wasm_bg.wasm.d.ts").write_text(
                "export {};\n", encoding="utf-8"
            )
        return generated

    def test_requires_explicit_name_approval(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "approval"):
                ASSEMBLER.assemble(
                    name="@gungorbasa/retrievalkit-browser",
                    version="0.1.0",
                    generated_root=root,
                    output=root / "output",
                )

    def test_rejects_alternate_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "exactly"):
                ASSEMBLER.assemble(
                    name="@example/retrievalkit-browser",
                    version="0.1.0",
                    generated_root=root,
                    output=root / "output",
                    name_approved=True,
                )

    def test_assembles_closed_self_contained_tarball(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inventory = ASSEMBLER.assemble(
                name="@gungorbasa/retrievalkit-browser",
                version="0.1.0",
                generated_root=self.write_generated(root),
                output=root / "output",
                name_approved=True,
                skip_build=True,
                package_root=self.write_package(root),
            )
            artifact = inventory["artifacts"][0]
            self.assertEqual(artifact["capability"], "browser-retrieval-graph")
            self.assertEqual(artifact["wasmTiers"], ["portable", "simd128"])
            self.assertEqual(
                artifact["file"], "gungorbasa-retrievalkit-browser-0.1.0.tgz"
            )
            self.assertTrue((root / "output" / artifact["file"]).is_file())
            VALIDATOR.validate_browser_retrieval_package(
                root / "output",
                VALIDATOR.load_json(
                    Path(__file__).resolve().parents[2] / "release/release-v0.1.0.json"
                ),
            )

    def test_rejects_missing_generated_tier(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generated = self.write_generated(root)
            (generated / "simd128/retrievalkit_wasm_bg.wasm").unlink()
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "missing"):
                ASSEMBLER.assemble(
                    name="@gungorbasa/retrievalkit-browser",
                    version="0.1.0",
                    generated_root=generated,
                    output=root / "output",
                    name_approved=True,
                    skip_build=True,
                    package_root=self.write_package(root),
                )

    def test_rejects_unexpected_package_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package_root = self.write_package(root)
            (package_root / "dist/unapproved.js").write_text(
                "export {};\n", encoding="utf-8"
            )
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "unexpected"):
                ASSEMBLER.assemble(
                    name="@gungorbasa/retrievalkit-browser",
                    version="0.1.0",
                    generated_root=self.write_generated(root),
                    output=root / "output",
                    name_approved=True,
                    skip_build=True,
                    skip_smoke=True,
                    package_root=package_root,
                )


if __name__ == "__main__":
    unittest.main()
