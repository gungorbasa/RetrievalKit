#!/usr/bin/env python3
"""Assemble and inspect Kotlin/JVM and Android Maven release artifacts."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path
from typing import Any
from xml.etree import ElementTree


REPO_ROOT = Path(__file__).resolve().parents[2]
KOTLIN_ROOT = REPO_ROOT / "wrappers" / "kotlin"
ARTIFACTS = {
    "jvm-base": ("retrievalkit", "jar"),
    "jvm-graph": ("retrievalkit-graph", "jar"),
    "android-base": ("retrievalkit-android", "aar"),
    "android-graph": ("retrievalkit-graph-android", "aar"),
}
GRADLE_TASKS = [
    ":base:publishMavenPublicationToReleaseRepository",
    ":graph:publishMavenPublicationToReleaseRepository",
    ":android-base:publishReleasePublicationToReleaseRepository",
    ":android-graph:publishReleasePublicationToReleaseRepository",
]
MAVEN_GROUP = re.compile(r"^[a-z][a-z0-9_-]*(?:\.[a-z][a-z0-9_-]*)+$")
SEMVER = re.compile(
    r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
POM_NAMESPACE = {"m": "http://maven.apache.org/POM/4.0.0"}
APPROVED_MAVEN_GROUP = "io.github.gungorbasa"


class AssemblyError(RuntimeError):
    """A Maven coordinate or generated artifact did not pass validation."""


def run(
    command: list[str],
    *,
    cwd: Path = REPO_ROOT,
    environment: dict[str, str] | None = None,
) -> None:
    subprocess.run(command, cwd=cwd, env=environment, check=True)


def validate_group(group: str) -> str:
    if not MAVEN_GROUP.fullmatch(group):
        raise AssemblyError(
            f"invalid Maven group {group!r}; supply an owner-approved reverse-domain namespace"
        )
    return group


def validate_version(version: str) -> str:
    if not SEMVER.fullmatch(version):
        raise AssemblyError(
            f"invalid version {version!r}; release assembly requires a SemVer x.y.z value"
        )
    return version


def java_environment(java_home: Path | None) -> dict[str, str]:
    environment = dict(os.environ)
    if java_home is not None:
        java = java_home / "bin" / "java"
        if not java.is_file():
            raise AssemblyError(f"JDK java executable is missing: {java}")
        environment["JAVA_HOME"] = str(java_home)
        environment["PATH"] = f"{java_home / 'bin'}{os.pathsep}{environment.get('PATH', '')}"
    if not environment.get("ANDROID_HOME"):
        default_android_home = Path.home() / "Library" / "Android" / "sdk"
        if default_android_home.is_dir():
            environment["ANDROID_HOME"] = str(default_android_home)
    return environment


def clean_output(output: Path) -> None:
    resolved = output.resolve()
    if resolved in {Path("/"), REPO_ROOT.resolve(), KOTLIN_ROOT.resolve()}:
        raise AssemblyError(f"refusing to replace unsafe output directory {resolved}")
    if resolved.exists():
        shutil.rmtree(resolved)
    resolved.mkdir(parents=True)


def digest(path: Path, algorithm: str) -> str:
    checksum = hashlib.new(algorithm)
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def validate_macho_arm64(data: bytes, description: str) -> None:
    try:
        magic, cpu_type = struct.unpack("<II", data[:8])
    except struct.error as error:
        raise AssemblyError(f"{description} has a truncated Mach-O header") from error
    if magic != 0xFEEDFACF or cpu_type != 0x0100000C:
        raise AssemblyError(f"{description} is not a 64-bit arm64 Mach-O library")


def validate_elf_arm64(data: bytes, description: str) -> None:
    if len(data) < 20 or data[:4] != b"\x7fELF":
        raise AssemblyError(f"{description} is not an ELF library")
    byte_order = "<" if data[5] == 1 else ">" if data[5] == 2 else None
    if byte_order is None or struct.unpack(f"{byte_order}H", data[18:20])[0] != 183:
        raise AssemblyError(f"{description} is not an arm64 ELF library")


def zip_files(path: Path) -> set[str]:
    with zipfile.ZipFile(path) as archive:
        names = set(archive.namelist())
        if any(name.startswith("/") or ".." in Path(name).parts for name in names):
            raise AssemblyError(f"{path.name} contains an unsafe archive path")
        return names


def read_zip_member(path: Path, member: str) -> bytes:
    with zipfile.ZipFile(path) as archive:
        try:
            return archive.read(member)
        except KeyError as error:
            raise AssemblyError(f"{path.name} is missing {member}") from error


def validate_pom(
    path: Path,
    *,
    group: str,
    artifact_id: str,
    version: str,
    extension: str,
) -> None:
    root = ElementTree.parse(path).getroot()

    def text(name: str) -> str:
        element = root.find(f"m:{name}", POM_NAMESPACE)
        return "" if element is None or element.text is None else element.text

    if (text("groupId"), text("artifactId"), text("version")) != (
        group,
        artifact_id,
        version,
    ):
        raise AssemblyError(f"{path.name} contains unexpected Maven coordinates")
    if extension == "aar" and text("packaging") != "aar":
        raise AssemblyError(f"{path.name} must declare AAR packaging")
    for required in ("name", "description", "url", "licenses", "developers", "scm"):
        if root.find(f"m:{required}", POM_NAMESPACE) is None:
            raise AssemblyError(f"{path.name} is missing required POM metadata: {required}")
    license_name = root.findtext(
        "m:licenses/m:license/m:name", default="", namespaces=POM_NAMESPACE
    )
    if "Apache" not in license_name:
        raise AssemblyError(f"{path.name} does not declare the Apache license")
    if root.find("m:repositories", POM_NAMESPACE) is not None:
        raise AssemblyError(f"{path.name} must not embed external repositories")


def validate_jvm_artifact(path: Path, capability: str) -> None:
    names = zip_files(path)
    if not {"LICENSE", "NOTICE"}.issubset(names):
        raise AssemblyError(f"{path.name} must embed LICENSE and NOTICE")
    base_native = "native/macos-aarch64/libretrievalkit_jni.dylib"
    graph_native = "native/macos-aarch64/libretrievalkit_jni_graph.dylib"
    if capability == "jvm-base":
        if base_native not in names or graph_native in names:
            raise AssemblyError(f"{path.name} does not isolate the base native aggregate")
        if any("Graph" in name for name in names if name.endswith(".class")):
            raise AssemblyError(f"{path.name} contains graph classes")
        validate_macho_arm64(read_zip_member(path, base_native), base_native)
    else:
        if graph_native not in names or base_native in names:
            raise AssemblyError(f"{path.name} does not isolate the graph native aggregate")
        if not any("GraphDatabase" in name for name in names):
            raise AssemblyError(f"{path.name} is missing graph classes")
        validate_macho_arm64(read_zip_member(path, graph_native), graph_native)


def validate_android_artifact(path: Path, capability: str) -> None:
    names = zip_files(path)
    base_native = "jni/arm64-v8a/libretrievalkit_jni.so"
    graph_native = "jni/arm64-v8a/libretrievalkit_jni_graph.so"
    classes = read_zip_member(path, "classes.jar")
    with zipfile.ZipFile(io.BytesIO(classes)) as classes_archive:
        class_names = set(classes_archive.namelist())
        if not {"LICENSE", "NOTICE"}.issubset(class_names):
            raise AssemblyError(f"{path.name} classes.jar must embed LICENSE and NOTICE")
    if capability == "android-base":
        if base_native not in names or graph_native in names:
            raise AssemblyError(f"{path.name} does not isolate the base Android aggregate")
        if any("Graph" in name for name in class_names if name.endswith(".class")):
            raise AssemblyError(f"{path.name} contains graph classes")
        validate_elf_arm64(read_zip_member(path, base_native), base_native)
    else:
        if graph_native not in names or base_native in names:
            raise AssemblyError(f"{path.name} does not isolate the graph Android aggregate")
        if not any("GraphDatabase" in name for name in class_names):
            raise AssemblyError(f"{path.name} is missing graph classes")
        validate_elf_arm64(read_zip_member(path, graph_native), graph_native)


def copy_publication(
    *,
    source_repository: Path,
    destination_repository: Path,
    group: str,
    artifact_id: str,
    version: str,
    extension: str,
) -> list[Path]:
    relative_directory = Path(*group.split(".")) / artifact_id / version
    source = source_repository / relative_directory
    destination = destination_repository / relative_directory
    destination.mkdir(parents=True, exist_ok=True)
    filenames = [
        f"{artifact_id}-{version}.{extension}",
        f"{artifact_id}-{version}-sources.jar",
        f"{artifact_id}-{version}-javadoc.jar",
        f"{artifact_id}-{version}.pom",
    ]
    copied: list[Path] = []
    for filename in filenames:
        source_file = source / filename
        if not source_file.is_file():
            raise AssemblyError(f"Gradle publication is missing {source_file}")
        destination_file = destination / filename
        shutil.copy2(source_file, destination_file)
        copied.append(destination_file)
    return copied


def write_checksum_companions(files: list[Path]) -> None:
    for path in files:
        for algorithm in ("md5", "sha1", "sha256", "sha512"):
            path.with_name(f"{path.name}.{algorithm}").write_text(
                digest(path, algorithm) + "\n",
                encoding="ascii",
            )


def sign_files(files: list[Path], signing_key: str, environment: dict[str, str]) -> None:
    for path in files:
        run(
            [
                "gpg",
                "--batch",
                "--yes",
                "--armor",
                "--detach-sign",
                "--local-user",
                signing_key,
                "--output",
                str(path.with_name(f"{path.name}.asc")),
                str(path),
            ],
            environment=environment,
        )


def write_canonical_bundle(source: Path, destination: Path) -> None:
    files = sorted(path for path in source.rglob("*") if path.is_file())
    with zipfile.ZipFile(
        destination,
        "w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as archive:
        for path in files:
            relative = path.relative_to(source).as_posix()
            entry = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, path.read_bytes())


def assemble(
    *,
    group: str,
    version: str,
    output: Path,
    java_home: Path | None = None,
    skip_native_build: bool = False,
    signing_key: str | None = None,
    namespace_verified: bool = False,
) -> dict[str, Any]:
    validate_group(group)
    if group != APPROVED_MAVEN_GROUP:
        raise AssemblyError(
            f"Maven group must be exactly {APPROVED_MAVEN_GROUP!r} for this release"
        )
    validate_version(version)
    if platform.system() != "Darwin" or platform.machine() not in {"arm64", "aarch64"}:
        raise AssemblyError(
            "Kotlin release assembly currently supports only the authorized macOS arm64 "
            "JVM and Android arm64-v8a targets"
        )
    environment = java_environment(java_home)
    if not skip_native_build:
        run(["./scripts/build-native.sh", "all"], cwd=KOTLIN_ROOT, environment=environment)

    clean_output(output)
    maven_output = output / "maven"
    maven_output.mkdir()
    with tempfile.TemporaryDirectory(prefix="retrievalkit-kotlin-gradle-repo-") as temporary:
        gradle_repository = Path(temporary) / "repository"
        run(
            [
                "./gradlew",
                "--no-daemon",
                *GRADLE_TASKS,
                f"-PretrievalkitMavenGroup={group}",
                f"-PretrievalkitVersion={version}",
                f"-PretrievalkitMavenRepository={gradle_repository.as_uri()}",
            ],
            cwd=KOTLIN_ROOT,
            environment=environment,
        )

        primary_files: list[Path] = []
        inventory_artifacts: list[dict[str, Any]] = []
        for capability, (artifact_id, extension) in ARTIFACTS.items():
            copied = copy_publication(
                source_repository=gradle_repository,
                destination_repository=maven_output,
                group=group,
                artifact_id=artifact_id,
                version=version,
                extension=extension,
            )
            primary = next(path for path in copied if path.suffix == f".{extension}")
            pom = next(path for path in copied if path.suffix == ".pom")
            validate_pom(
                pom,
                group=group,
                artifact_id=artifact_id,
                version=version,
                extension=extension,
            )
            if extension == "jar":
                validate_jvm_artifact(primary, capability)
            else:
                validate_android_artifact(primary, capability)
            for classifier in ("sources", "javadoc"):
                classified = next(path for path in copied if f"-{classifier}.jar" in path.name)
                if not zipfile.is_zipfile(classified):
                    raise AssemblyError(f"{classified.name} is not a valid JAR")
            primary_files.extend(copied)
            inventory_artifacts.append(
                {
                    "capability": capability,
                    "coordinates": f"{group}:{artifact_id}:{version}",
                    "packaging": extension,
                    "files": sorted(path.name for path in copied),
                    "primarySha256": digest(primary, "sha256"),
                }
            )

    write_checksum_companions(primary_files)
    if signing_key:
        sign_files(primary_files, signing_key, environment)

    signed = signing_key is not None
    bundle_name = (
        f"retrievalkit-kotlin-{version}-central-bundle.zip"
        if signed
        else f"retrievalkit-kotlin-{version}-unsigned-central-bundle.zip"
    )
    bundle = output / bundle_name
    write_canonical_bundle(maven_output, bundle)
    blockers: list[str] = []
    if not namespace_verified:
        blockers.append(f"Central Portal ownership of Maven namespace {group!r} is unverified")
    if not signed:
        blockers.append("Central requires PGP signatures; no signing key was supplied")
    upload_blockers = [
        "A Central Portal user token is required to upload the deployment bundle"
    ]
    inventory_artifacts.sort(key=lambda artifact: artifact["capability"])
    inventory = {
        "schemaVersion": 1,
        "kind": "retrievalkit-kotlin-release",
        "group": group,
        "version": version,
        "targets": ["jvm-macos-arm64", "android-arm64-v8a"],
        "publicationReady": not blockers,
        "artifactBlockers": blockers,
        "uploadBlockers": upload_blockers,
        "bundle": {
            "file": bundle.name,
            "sha256": digest(bundle, "sha256"),
            "signed": signed,
        },
        "artifacts": inventory_artifacts,
    }
    (output / "inventory.json").write_text(
        json.dumps(inventory, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    release_files = sorted(
        path
        for path in output.rglob("*")
        if path.is_file() and path.name not in {"inventory.json", "SHA256SUMS"}
    )
    (output / "SHA256SUMS").write_text(
        "".join(
            f"{digest(path, 'sha256')}  {path.relative_to(output).as_posix()}\n"
            for path in release_files
        ),
        encoding="ascii",
    )
    return inventory


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build a Maven Central layout for all Kotlin capabilities. An explicit "
            "owner-approved group is required; namespace ownership is never inferred."
        )
    )
    parser.add_argument("--group", required=True, help="owner-approved Maven groupId")
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "dist" / "release" / "kotlin",
    )
    parser.add_argument("--java-home", type=Path)
    parser.add_argument("--skip-native-build", action="store_true")
    parser.add_argument("--signing-key", help="GPG key ID used for detached ASCII signatures")
    parser.add_argument(
        "--namespace-verified",
        action="store_true",
        help="assert that the owner controls the fixed Maven namespace",
    )
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    try:
        inventory = assemble(
            group=arguments.group,
            version=arguments.version,
            output=arguments.output,
            java_home=arguments.java_home,
            skip_native_build=arguments.skip_native_build,
            signing_key=arguments.signing_key,
            namespace_verified=arguments.namespace_verified,
        )
    except (AssemblyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Kotlin package assembly failed: {error}") from error
    print(
        f"Assembled {len(inventory['artifacts'])} verified Maven artifacts in "
        f"{arguments.output.resolve()}"
    )
    if inventory["artifactBlockers"]:
        print("Publication remains fail-closed:")
        for blocker in inventory["artifactBlockers"]:
            print(f"- {blocker}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
