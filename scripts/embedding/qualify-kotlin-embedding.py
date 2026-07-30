#!/usr/bin/env python3
"""Run frozen Kotlin/JVM embedding conformance and the fixed 50/750 benchmark.

The harness compiles a temporary Java driver against the already-built Kotlin
embedding JAR. Model acquisition is disabled by default. An explicit
``--download-if-missing`` run measures verified prefetch separately, then
loads locally from the resulting cache. ``--packaged-libraries`` exercises
the production JAR's native-library extraction path.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

WARMUPS = 50
MEASURED = 750

DRIVER_SOURCE = r"""
import ai.retrievalkit.embedding.EmbeddingModelInfo;
import ai.retrievalkit.embedding.OnnxEmbedder;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public final class KotlinEmbeddingQualification {
  private static final int WARMUPS = 50;
  private static final int MEASURED = 750;
  private static final String BENCHMARK_TEXT =
      "token0 token1 token2 token3 token4 token5 token6 token7 " +
      "token8 token9 token10 token11 token12 token13 token14 token15 " +
      "token16 token17 token18 token19 token20 token21 token22 token23 " +
      "token24 token25 token26 token27 token28 token29 token30 token31";

  public static void main(String[] args) throws Exception {
    if (args.length != 9) {
      throw new IllegalArgumentException(
          "expected native, runtime, cache, input, vectors, benchmark, threads, acquisition, localOnly");
    }
    if (!args[0].equals("-")) {
      System.setProperty("retrievalkit.embedding.native.path", args[0]);
    }
    File runtime = args[1].equals("-") ? null : new File(args[1]);
    File cache = new File(args[2]);
    List<String> ids = new ArrayList<>();
    List<String> texts = new ArrayList<>();
    for (String row : Files.readAllLines(Path.of(args[3]), StandardCharsets.UTF_8)) {
      String[] parts = row.split("\t", 2);
      if (parts.length != 2) throw new IllegalArgumentException("invalid input row");
      ids.add(new String(Base64.getDecoder().decode(parts[0]), StandardCharsets.UTF_8));
      texts.add(new String(Base64.getDecoder().decode(parts[1]), StandardCharsets.UTF_8));
    }
    int threads = Integer.parseInt(args[6]);
    boolean acquire = Boolean.parseBoolean(args[7]);
    boolean localOnly = Boolean.parseBoolean(args[8]);
    double acquisitionMs = -1.0;
    if (acquire) {
      long acquisitionStart = System.nanoTime();
      OnnxEmbedder.prefetch(cache, localOnly);
      acquisitionMs = elapsedMs(acquisitionStart);
      localOnly = true;
    }
    long loadStart = System.nanoTime();
    try (OnnxEmbedder embedder =
        OnnxEmbedder.load(localOnly, cache, runtime, threads, 1)) {
      double loadMs = elapsedMs(loadStart);
      long firstStart = System.nanoTime();
      float[] first = embedder.embed(BENCHMARK_TEXT);
      double firstMs = elapsedMs(firstStart);
      requireVector(first);

      List<float[]> vectors = embedder.embedBatch(texts);
      if (vectors.size() != texts.size()) throw new IllegalStateException("vector count mismatch");
      writeConformance(Path.of(args[4]), ids, vectors, embedder.getModelInfo());

      for (int i = 0; i < WARMUPS; i++) requireVector(embedder.embed(BENCHMARK_TEXT));
      double[] samples = new double[MEASURED];
      for (int i = 0; i < MEASURED; i++) {
        long started = System.nanoTime();
        requireVector(embedder.embed(BENCHMARK_TEXT));
        samples[i] = elapsedMs(started);
      }
      Arrays.sort(samples);
      writeBenchmark(
          Path.of(args[5]), embedder.getModelInfo(), acquisitionMs, loadMs, firstMs, samples, threads);
    }
  }

  private static void requireVector(float[] vector) {
    if (vector.length != 384) throw new IllegalStateException("expected 384 values");
    double norm = 0.0;
    for (float value : vector) {
      if (!Float.isFinite(value)) throw new IllegalStateException("non-finite value");
      norm += (double)value * value;
    }
    if (Math.abs(Math.sqrt(norm) - 1.0) > 1e-4) {
      throw new IllegalStateException("embedding is not normalized");
    }
  }

  private static double elapsedMs(long started) {
    return (System.nanoTime() - started) / 1_000_000.0;
  }

  private static double percentile(double[] sorted, double fraction) {
    int index = Math.max(0, (int)Math.ceil(sorted.length * fraction) - 1);
    return sorted[index];
  }

  private static String quote(String value) {
    StringBuilder output = new StringBuilder("\"");
    for (int i = 0; i < value.length(); i++) {
      char c = value.charAt(i);
      switch (c) {
        case '"': output.append("\\\""); break;
        case '\\': output.append("\\\\"); break;
        case '\b': output.append("\\b"); break;
        case '\f': output.append("\\f"); break;
        case '\n': output.append("\\n"); break;
        case '\r': output.append("\\r"); break;
        case '\t': output.append("\\t"); break;
        default:
          if (c < 0x20) output.append(String.format("\\u%04x", (int)c));
          else output.append(c);
      }
    }
    return output.append('"').toString();
  }

  private static String model(EmbeddingModelInfo info, boolean conformance) {
    return "\"identifier\":" + quote(info.getIdentifier()) +
        ",\"revision\":" + quote(info.getRevision()) +
        ",\"profile\":\"fp32\"" +
        (conformance ? ",\"dtype\":\"float32\"" : "") +
        ",\"dimension\":" + info.getDimension() +
        ",\"max_input_tokens\":" + info.getMaxInputTokens() +
        (conformance ? ",\"normalized\":true" :
          ",\"produces_normalized_embeddings\":" + info.getProducesNormalizedEmbeddings() +
          ",\"runtime_version\":" + quote(info.getRuntimeVersion()));
  }

  private static void writeConformance(
      Path output, List<String> ids, List<float[]> vectors, EmbeddingModelInfo info)
      throws IOException {
    StringBuilder json = new StringBuilder(
        "{\"schema_version\":1,\"model\":{" + model(info, true) + "},\"items\":[");
    for (int row = 0; row < vectors.size(); row++) {
      if (row > 0) json.append(',');
      json.append("{\"id\":").append(quote(ids.get(row))).append(",\"embedding\":[");
      float[] vector = vectors.get(row);
      requireVector(vector);
      for (int column = 0; column < vector.length; column++) {
        if (column > 0) json.append(',');
        json.append(Float.toString(vector[column]));
      }
      json.append("]}");
    }
    json.append("]}\n");
    Files.createDirectories(output.toAbsolutePath().getParent());
    Files.writeString(output, json, StandardCharsets.UTF_8);
  }

  private static void writeBenchmark(
      Path output, EmbeddingModelInfo info, double acquisitionMs, double loadMs, double firstMs,
      double[] samples, int threads) throws IOException {
    String json = "{\"provider\":\"kotlin-jni-onnx-fp32\",\"build_mode\":\"release\"" +
        ",\"model\":{" + model(info, false) + "}" +
        ",\"token_length\":32,\"warmups\":" + WARMUPS +
        ",\"measured\":" + MEASURED + ",\"intra_threads\":" + threads +
        ",\"inter_threads\":1,\"verified_prefetch_ms\":" +
        (acquisitionMs < 0.0 ? "null" : Double.toString(acquisitionMs)) +
        ",\"load_ms\":" + loadMs +
        ",\"first_inference_ms\":" + firstMs +
        ",\"warm_embedding_ms\":{\"p50\":" + percentile(samples, 0.50) +
        ",\"p95\":" + percentile(samples, 0.95) +
        ",\"p99\":" + percentile(samples, 0.99) +
        ",\"min\":" + samples[0] + ",\"max\":" + samples[samples.length - 1] + "}}\n";
    Files.createDirectories(output.toAbsolutePath().getParent());
    Files.writeString(output, json, StandardCharsets.UTF_8);
  }
}
"""


class QualificationError(ValueError):
    """Qualification inputs or the local JVM toolchain are incomplete."""


def load_input(path: Path) -> list[tuple[str, str]]:
    try:
        document: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise QualificationError(f"cannot read qualification input: {error}") from error
    if not isinstance(document, dict) or document.get("schema_version") != 1:
        raise QualificationError("input must be a schema_version 1 object")
    items = document.get("items")
    if not isinstance(items, list) or not items:
        raise QualificationError("input.items must be a non-empty array")
    result: list[tuple[str, str]] = []
    identifiers: set[str] = set()
    for index, item in enumerate(items):
        if not isinstance(item, dict):
            raise QualificationError(f"input.items[{index}] must be an object")
        identifier = item.get("id")
        text = item.get("text")
        if not isinstance(identifier, str) or not identifier:
            raise QualificationError(f"input.items[{index}].id must be non-empty")
        if identifier in identifiers:
            raise QualificationError(f"duplicate input ID: {identifier}")
        if not isinstance(text, str) or not text.strip():
            raise QualificationError(f"input.items[{index}].text must be non-blank")
        identifiers.add(identifier)
        result.append((identifier, text))
    return result


def require_file(path: Path, label: str) -> Path:
    resolved = path.resolve()
    if not resolved.is_file():
        raise QualificationError(f"{label} is not a regular file: {resolved}")
    return resolved


def find_java_tool(name: str, java_home: Path | None) -> str:
    if java_home is not None:
        candidate = java_home.resolve() / "bin" / name
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    found = shutil.which(name)
    if found is None:
        raise QualificationError(f"required JVM tool is missing: {name}")
    return found


def find_kotlin_stdlib(explicit: Path | None) -> Path:
    if explicit is not None:
        return require_file(explicit, "Kotlin standard library")
    candidates = sorted(
        Path.home().glob(
            ".gradle/caches/modules-2/files-2.1/org.jetbrains.kotlin/"
            "kotlin-stdlib/1.9.22/*/kotlin-stdlib-1.9.22.jar"
        )
    )
    if len(candidates) != 1:
        raise QualificationError(
            "could not identify exactly one Kotlin 1.9.22 stdlib; pass --kotlin-stdlib"
        )
    return candidates[0].resolve()


def run(args: argparse.Namespace) -> None:
    rows = load_input(args.input.resolve())
    jar = require_file(args.embedding_jar, "embedding JAR")
    if args.packaged_libraries:
        native_argument = "-"
        runtime_argument = "-"
    else:
        if args.native_library is None or args.runtime_library is None:
            raise QualificationError(
                "--native-library and --runtime-library are required unless "
                "--packaged-libraries is selected"
            )
        native_argument = str(
            require_file(args.native_library, "Kotlin embedding JNI library")
        )
        runtime_argument = str(
            require_file(args.runtime_library, "ONNX Runtime library")
        )
    if not args.download_if_missing and not args.cache_directory.resolve().is_dir():
        raise QualificationError(
            f"verified local cache is missing: {args.cache_directory.resolve()}"
        )
    stdlib = find_kotlin_stdlib(args.kotlin_stdlib)
    javac = find_java_tool("javac", args.java_home)
    java = find_java_tool("java", args.java_home)

    args.output.resolve().parent.mkdir(parents=True, exist_ok=True)
    args.benchmark_output.resolve().parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="retrievalkit-kotlin-qualification-") as directory:
        temporary = Path(directory)
        source = temporary / "KotlinEmbeddingQualification.java"
        source.write_text(DRIVER_SOURCE, encoding="utf-8", newline="\n")
        input_rows = temporary / "input.tsv"
        input_rows.write_text(
            "".join(
                f"{base64.b64encode(identifier.encode()).decode()}\t"
                f"{base64.b64encode(text.encode()).decode()}\n"
                for identifier, text in rows
            ),
            encoding="ascii",
            newline="\n",
        )
        classpath = os.pathsep.join((str(jar), str(stdlib)))
        subprocess.run(
            [javac, "-encoding", "UTF-8", "-cp", classpath, str(source)],
            check=True,
        )
        subprocess.run(
            [
                java,
                "-cp",
                os.pathsep.join((str(temporary), classpath)),
                "KotlinEmbeddingQualification",
                native_argument,
                runtime_argument,
                str(args.cache_directory.resolve()),
                str(input_rows),
                str(args.output.resolve()),
                str(args.benchmark_output.resolve()),
                str(args.intra_threads),
                str(args.download_if_missing).lower(),
                str(not args.download_if_missing).lower(),
            ],
            check=True,
        )


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--input", type=Path, required=True)
    result.add_argument("--output", type=Path, required=True)
    result.add_argument("--benchmark-output", type=Path, required=True)
    result.add_argument("--embedding-jar", type=Path, required=True)
    result.add_argument("--native-library", type=Path)
    result.add_argument("--runtime-library", type=Path)
    result.add_argument(
        "--packaged-libraries",
        action="store_true",
        help="exercise the JNI and ONNX Runtime resources embedded in the JAR",
    )
    result.add_argument(
        "--download-if-missing",
        action="store_true",
        help="allow verified prefetch into the selected cache, then load locally",
    )
    result.add_argument("--cache-directory", type=Path, required=True)
    result.add_argument("--kotlin-stdlib", type=Path)
    result.add_argument("--java-home", type=Path)
    result.add_argument("--intra-threads", type=int, default=4)
    return result


def main() -> int:
    arguments = parser().parse_args()
    if arguments.intra_threads < 1:
        parser().error("--intra-threads must be at least one")
    if arguments.output.resolve() == arguments.benchmark_output.resolve():
        parser().error("--output and --benchmark-output must differ")
    try:
        run(arguments)
    except QualificationError as error:
        parser().error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
