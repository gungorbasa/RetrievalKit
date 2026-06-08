from __future__ import annotations

import argparse
import json
import shutil
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from fastembed import TextEmbedding
from vectorkit import Index


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_JSON_PATH = Path("/Users/gungorbasa/Desktop/the_social_network_v.1.32.json")
DEFAULT_INDEX_DIR = ROOT_DIR / "target" / "examples" / "social-network-index"
DEFAULT_MODEL = "BAAI/bge-small-en-v1.5"
EXPECTED_DIMENSION = 384


@dataclass(frozen=True)
class SearchRecord:
    document_id: str
    text: str
    metadata: dict[str, str | int | float | bool]


def main() -> None:
    args = parse_args()
    source_path = Path(args.json)
    index_dir = Path(args.index_dir)
    model_start = time.perf_counter()
    model = TextEmbedding(model_name=args.model)
    model_init_ms = elapsed_ms(model_start)

    if args.rebuild or not (index_dir / "manifest.json").exists():
        records_start = time.perf_counter()
        records = build_records(source_path, max_text_chars=args.max_text_chars)
        record_prep_ms = elapsed_ms(records_start)
        if args.max_records:
            records = records[: args.max_records]
        build_index(records, index_dir, model, args.batch_size, record_prep_ms)

    load_start = time.perf_counter()
    index = Index.load(index_dir)
    load_ms = elapsed_ms(load_start)

    embed_start = time.perf_counter()
    query_embedding = embed_one(model, args.query)
    query_embed_ms = elapsed_ms(embed_start)

    where = {"kind": args.where_kind} if args.where_kind else None
    search_start = time.perf_counter()
    hits = index.search(query_embedding, limit=args.limit, where=where)
    search_ms = elapsed_ms(search_start)
    print_hits(args.query, hits)
    print_one_shot_timings(model_init_ms, load_ms, query_embed_ms, search_ms)

    if args.timing_runs > 0:
        print_timing_report(
            index_dir=index_dir,
            model=model,
            query=args.query,
            query_embedding=query_embedding,
            limit=args.limit,
            where=where,
            timing_runs=args.timing_runs,
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build and search a VectorKit index for The Social Network JSON analysis."
    )
    parser.add_argument("--json", default=str(DEFAULT_JSON_PATH), help="source JSON path")
    parser.add_argument(
        "--index-dir",
        default=str(DEFAULT_INDEX_DIR),
        help="directory where the VectorKit index is saved",
    )
    parser.add_argument(
        "--query",
        default="Mark and Erica arguing in a dim bar",
        help="query text to embed and search",
    )
    parser.add_argument("--limit", type=int, default=5, help="number of hits to return")
    parser.add_argument(
        "--where-kind",
        choices=["scene", "shot"],
        help="optional metadata filter for scene or shot chunks",
    )
    parser.add_argument(
        "--model",
        default=DEFAULT_MODEL,
        help="FastEmbed model name; must return 384-dimensional vectors for this example",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=64,
        help="embedding/indexing batch size",
    )
    parser.add_argument(
        "--max-text-chars",
        type=int,
        default=6000,
        help="maximum characters included in one searchable chunk",
    )
    parser.add_argument(
        "--max-records",
        type=int,
        default=0,
        help="optional smoke-test limit; 0 means index all scene and shot records",
    )
    parser.add_argument(
        "--rebuild",
        action="store_true",
        help="delete and rebuild the saved index before searching",
    )
    parser.add_argument(
        "--timing-runs",
        type=int,
        default=20,
        help="number of repeated runs for average load/embed/search timing; 0 disables",
    )
    return parser.parse_args()


def build_records(source_path: Path, *, max_text_chars: int) -> list[SearchRecord]:
    with source_path.open("r", encoding="utf-8") as file:
        data = json.load(file)

    media_name = scalar_string(data.get("metadata", {}).get("media_name"), "unknown")
    records: list[SearchRecord] = []

    for scene_id, scene in sorted(data.get("scenes", {}).items()):
        metadata = scene_metadata(media_name, scene)
        text = scene_text(scene, max_text_chars=max_text_chars)
        if text:
            records.append(SearchRecord(f"scene:{scene_id}", text, metadata))

    for shot_id, shot in sorted(data.get("shots", {}).items()):
        metadata = shot_metadata(media_name, shot)
        text = shot_text(shot, max_text_chars=max_text_chars)
        if text:
            records.append(SearchRecord(f"shot:{shot_id}", text, metadata))

    print(f"Prepared {len(records)} records from {source_path}")
    return records


def build_index(
    records: list[SearchRecord],
    index_dir: Path,
    model: TextEmbedding,
    batch_size: int,
    record_prep_ms: float,
) -> None:
    build_start = time.perf_counter()
    if index_dir.exists():
        shutil.rmtree(index_dir)

    index = Index(dimension=EXPECTED_DIMENSION, metric="cosine", encoding="i8")
    total_embed_ms = 0.0
    total_add_ms = 0.0

    for batch_number, batch in enumerate(batches(records, batch_size), start=1):
        texts = [record.text for record in batch]
        embed_start = time.perf_counter()
        embeddings = list(model.embed(texts, batch_size=batch_size))
        total_embed_ms += elapsed_ms(embed_start)
        documents = []

        for record, embedding in zip(batch, embeddings):
            vector = embedding.tolist()
            validate_dimension(vector, record.document_id)
            documents.append(
                {
                    "id": record.document_id,
                    "metadata": record.metadata,
                    "chunks": [
                        {
                            "text": record.text,
                            "embedding": vector,
                        }
                    ],
                }
            )

        add_start = time.perf_counter()
        index.add(documents=documents)
        total_add_ms += elapsed_ms(add_start)
        print(f"Indexed batch {batch_number}: {len(documents)} records")

    save_start = time.perf_counter()
    report = index.save(index_dir)
    save_ms = elapsed_ms(save_start)
    build_ms = elapsed_ms(build_start)

    print(f"Saved index to {index_dir}")
    print(f"Persisted size: {report['total_bytes'] / (1024 * 1024):.3f} MiB")
    print("\nBuild timing:")
    print(f"  record_prep_ms: {record_prep_ms:.3f}")
    print(f"  embedding_total_ms: {total_embed_ms:.3f}")
    print(f"  index_add_total_ms: {total_add_ms:.3f}")
    print(f"  save_ms: {save_ms:.3f}")
    print(f"  build_total_ms: {build_ms:.3f}")


def embed_one(model: TextEmbedding, text: str) -> list[float]:
    embeddings = list(model.embed([text], batch_size=1))
    if len(embeddings) != 1:
        raise RuntimeError("embedding provider did not return exactly one query embedding")
    vector = embeddings[0].tolist()
    validate_dimension(vector, "query")
    return vector


def validate_dimension(vector: list[float], label: str) -> None:
    if len(vector) != EXPECTED_DIMENSION:
        raise ValueError(
            f"{label} embedding has dimension {len(vector)}, expected {EXPECTED_DIMENSION}"
        )


def scene_metadata(
    media_name: str,
    scene: dict[str, Any],
) -> dict[str, str | int | float | bool]:
    structure = scene.get("structure", {})
    timing = scene.get("timing", {})
    location = get_path(scene, "location_description", "location_info")
    temporal = get_path(scene, "temporal_description", "temporal_info")
    dialogue = get_path(scene, "dialogue_description", "dialogue_info")
    return clean_metadata(
        {
            "media_name": media_name,
            "kind": "scene",
            "scene_number": structure.get("scene_number"),
            "shot_count": structure.get("shot_count"),
            "start_time": timing.get("start_time"),
            "end_time": timing.get("end_time"),
            "duration": timing.get("duration"),
            "location_name": get_path(location, "location_name"),
            "time_of_day": get_path(temporal, "time_of_day"),
            "has_dialogue": get_path(dialogue, "has_dialogue"),
        }
    )


def shot_metadata(
    media_name: str,
    shot: dict[str, Any],
) -> dict[str, str | int | float | bool]:
    structure = shot.get("structure", {})
    timing = shot.get("timing", {})
    return clean_metadata(
        {
            "media_name": media_name,
            "kind": "shot",
            "shot_number": structure.get("shot_number"),
            "global_shot_number": structure.get("global_shot_number"),
            "start_time": timing.get("start_time"),
            "end_time": timing.get("end_time"),
            "duration": timing.get("duration"),
            "location_name": get_path(shot, "location_info", "location_name"),
            "time_of_day": get_path(shot, "temporal_info", "time_of_day"),
            "has_dialogue": get_path(shot, "dialogue_info", "has_dialogue"),
        }
    )


def scene_text(scene: dict[str, Any], *, max_text_chars: int) -> str:
    parts = [
        text_part("Scene summary", get_path(scene, "story_description", "summary")),
        text_part(
            "Narrative",
            get_path(scene, "video_description", "brief_scene_narrative_description"),
        ),
        text_part(
            "People",
            get_path(scene, "people_description", "people_info", "people_description"),
        ),
        text_part(
            "Location",
            get_path(scene, "location_description", "location_info", "location_description"),
        ),
        text_part(
            "Dialogue",
            get_path(scene, "dialogue_description", "dialogue_info", "dialogue_description"),
        ),
        text_part(
            "Emotion",
            get_path(scene, "emotions_description", "emotion_info", "emotion_description"),
        ),
        text_part(
            "Visuals",
            get_path(scene, "visual_description", "visual_info", "visual_description"),
        ),
        text_part("Characters", character_analysis_summary(scene.get("character_analysis"))),
    ]
    return trim_text("\n".join(part for part in parts if part), max_text_chars)


def shot_text(shot: dict[str, Any], *, max_text_chars: int) -> str:
    parts = [
        text_part(
            "Shot narrative",
            get_path(shot, "video_description", "brief_shot_narrative_description"),
        ),
        text_part("People", get_path(shot, "people_info", "people_description")),
        text_part("Objects", get_path(shot, "object_info", "object_description")),
        text_part("Location", get_path(shot, "location_info", "location_description")),
        text_part("Dialogue", get_path(shot, "dialogue_info", "dialogue_description")),
        text_part("Dialogue lines", dialogue_lines(shot.get("dialogue_info"))),
        text_part("Emotion", get_path(shot, "emotion_info", "emotion_description")),
        text_part("Visuals", get_path(shot, "visual_info", "visual_description")),
    ]
    return trim_text("\n".join(part for part in parts if part), max_text_chars)


def text_part(label: str, value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        value = value.strip()
    if not value:
        return ""
    return f"{label}: {value}"


def dialogue_lines(dialogue_info: Any, *, limit: int = 20) -> str:
    if not isinstance(dialogue_info, dict):
        return ""
    segments = dialogue_info.get("cc_segments") or dialogue_info.get("dialogue_segments") or []
    lines = []
    for segment in segments[:limit]:
        if isinstance(segment, dict):
            text = segment.get("ref") or segment.get("text")
            speaker = segment.get("name") or segment.get("character_name")
            if text:
                lines.append(f"{speaker}: {text}" if speaker else str(text))
    return " ".join(lines)


def character_analysis_summary(value: Any, *, per_character_chars: int = 500) -> str:
    if not isinstance(value, dict):
        return ""
    parts = []
    for character, analysis in value.items():
        if not isinstance(analysis, dict):
            continue
        individual = scalar_string(analysis.get("individual"), "")
        progressive = scalar_string(analysis.get("progressive"), "")
        summary = f"{individual} {progressive}".strip()
        if summary:
            parts.append(f"{character}: {summary[:per_character_chars]}")
    return " ".join(parts)


def trim_text(text: str, max_chars: int) -> str:
    normalized = " ".join(text.split())
    return normalized[:max_chars]


def get_path(value: Any, *path: str) -> Any:
    current = value
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def clean_metadata(values: dict[str, Any]) -> dict[str, str | int | float | bool]:
    metadata: dict[str, str | int | float | bool] = {}
    for key, value in values.items():
        if value is None:
            continue
        if isinstance(value, bool):
            metadata[key] = value
        elif isinstance(value, int):
            metadata[key] = value
        elif isinstance(value, float):
            metadata[key] = value
        elif isinstance(value, str) and value:
            metadata[key] = value
    return metadata


def scalar_string(value: Any, default: str) -> str:
    return value if isinstance(value, str) and value else default


def batches(records: list[SearchRecord], size: int) -> Iterable[list[SearchRecord]]:
    for index in range(0, len(records), size):
        yield records[index : index + size]


def print_hits(query: str, hits: list[dict[str, Any]]) -> None:
    print(f"\nQuery: {query}")
    if not hits:
        print("No hits.")
        return

    for rank, hit in enumerate(hits, start=1):
        metadata = hit["metadata"]
        start = metadata.get("start_time", "?")
        end = metadata.get("end_time", "?")
        label = hit["document_id"]
        print(f"\n{rank}. {label} [{metadata.get('kind')}] score={hit['score']:.4f}")
        print(f"   time={start}-{end} location={metadata.get('location_name', 'unknown')}")
        print(f"   {hit['text'][:500]}...")


def print_one_shot_timings(
    model_init_ms: float,
    load_ms: float,
    query_embed_ms: float,
    search_ms: float,
) -> None:
    print("\nOne-shot timing:")
    print(f"  model_init_ms: {model_init_ms:.3f}")
    print(f"  load_ms: {load_ms:.3f}")
    print(f"  query_embed_ms: {query_embed_ms:.3f}")
    print(f"  vector_search_ms: {search_ms:.3f}")
    print(f"  semantic_query_total_ms: {query_embed_ms + search_ms:.3f}")


def print_timing_report(
    *,
    index_dir: Path,
    model: TextEmbedding,
    query: str,
    query_embedding: list[float],
    limit: int,
    where: dict[str, Any] | None,
    timing_runs: int,
) -> None:
    load_times = []
    query_embed_times = []
    vector_search_times = []
    keyword_search_times = []
    semantic_total_times = []

    for _ in range(timing_runs):
        start = time.perf_counter()
        loaded = Index.load(index_dir)
        load_times.append(elapsed_ms(start))

    index = Index.load(index_dir)

    for _ in range(timing_runs):
        start = time.perf_counter()
        vector = embed_one(model, query)
        query_embed_times.append(elapsed_ms(start))

        start = time.perf_counter()
        index.search(vector, limit=limit, where=where)
        search_ms = elapsed_ms(start)
        semantic_total_times.append(query_embed_times[-1] + search_ms)

    for _ in range(timing_runs):
        start = time.perf_counter()
        index.search(query_embedding, limit=limit, where=where)
        vector_search_times.append(elapsed_ms(start))

    for _ in range(timing_runs):
        start = time.perf_counter()
        index.keyword_search(query, limit=limit, where=where)
        keyword_search_times.append(elapsed_ms(start))

    print(f"\nAverage timing over {timing_runs} runs:")
    print_stats("load_ms", load_times)
    print_stats("query_embed_ms", query_embed_times)
    print_stats("vector_search_ms", vector_search_times)
    print_stats("keyword_search_ms", keyword_search_times)
    print_stats("semantic_query_total_ms", semantic_total_times)


def print_stats(label: str, values: list[float]) -> None:
    sorted_values = sorted(values)
    avg = sum(values) / len(values)
    p50 = percentile(sorted_values, 0.50)
    p95 = percentile(sorted_values, 0.95)
    print(
        f"  {label}: avg={avg:.3f} p50={p50:.3f} p95={p95:.3f} "
        f"min={min(values):.3f} max={max(values):.3f}"
    )


def percentile(sorted_values: list[float], quantile: float) -> float:
    if not sorted_values:
        return 0.0
    index = min(len(sorted_values) - 1, int((len(sorted_values) - 1) * quantile))
    return sorted_values[index]


def elapsed_ms(start: float) -> float:
    return (time.perf_counter() - start) * 1000.0


if __name__ == "__main__":
    main()
