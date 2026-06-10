from __future__ import annotations

import argparse
import json
import shutil
import time
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, Iterable, TypeAlias

import yaml
from fastembed import TextEmbedding
from vectorkit import Index
from vectorkit import where as vk_where

if TYPE_CHECKING:
    from vectorkit import DocumentInput, Filter, HybridHit, KeywordHit, SearchHit
else:
    DocumentInput: TypeAlias = dict[str, Any]
    Filter: TypeAlias = dict[str, Any]
    SearchHit: TypeAlias = dict[str, Any]
    KeywordHit: TypeAlias = dict[str, Any]
    HybridHit: TypeAlias = dict[str, Any]


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_JSON_PATH = Path("/Users/gungorbasa/Desktop/the_social_network_v.1.32.json")
DEFAULT_INDEX_DIR = ROOT_DIR / "target" / "examples" / "social-network-index"
DEFAULT_MODEL = "BAAI/bge-small-en-v1.5"
EXPECTED_DIMENSION = 384
DEFAULT_CHUNK_TOKEN_LIMIT = 500
DEFAULT_CHUNK_OVERLAP = 2
SCENE_DESCRIPTION_FIELDS = [
    "people_description",
    "object_description",
    "location_description",
    "temporal_description",
    "audio_description",
    "emotions_description",
    "visual_description",
    "video_description",
    "story_description",
]
SHOT_SECTIONS = [
    ("location", "LOCATION INFO", "location_info"),
    ("temporal", "TEMPORAL INFO", "temporal_info"),
    ("people", "PEOPLE INFO", "people_info"),
    ("objects", "OBJECT INFO", "object_info"),
    ("emotion", "EMOTION INFO", "emotion_info"),
    ("audio", "AUDIO INFO", "audio_info"),
    ("visual", "VISUAL INFO", "visual_info"),
    ("video", "VIDEO DESCRIPTION", "video_description"),
]


@dataclass(frozen=True)
class SearchRecord:
    document_id: str
    text: str
    metadata: dict[str, str | int | float | bool]


@dataclass(frozen=True)
class SearchRun:
    mode: str
    hits: list[SearchHit] | list[KeywordHit] | list[HybridHit]
    search_ms: float


def main() -> None:
    args = parse_args()
    source_path = Path(args.json)
    index_dir = Path(args.index_dir)
    model_start = time.perf_counter()
    model = TextEmbedding(model_name=args.model)
    model_init_ms = elapsed_ms(model_start)

    if args.rebuild or not (index_dir / "manifest.json").exists():
        records_start = time.perf_counter()
        records = build_records(
            source_path,
            chunk_token_limit=args.chunk_token_limit,
            chunk_overlap=args.chunk_overlap,
        )
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

    filters = build_filter(
        kind=args.where_kind,
        start_time=args.start_time,
        end_time=args.end_time,
        time_filter_mode=args.time_filter_mode,
    )
    search_runs = run_searches(
        index=index,
        mode=args.search_mode,
        query=args.query,
        query_embedding=query_embedding,
        limit=args.limit,
        filters=filters,
        fusion=args.fusion,
        vector_candidates=args.vector_candidates,
        keyword_candidates=args.keyword_candidates,
        vector_weight=args.vector_weight,
        keyword_weight=args.keyword_weight,
        rrf_k=args.rrf_k,
    )
    for run in search_runs:
        print_hits(args.query, run.mode, run.hits)
    print_one_shot_timings(model_init_ms, load_ms, query_embed_ms, search_runs)

    if args.timing_runs > 0:
        print_timing_report(
            index_dir=index_dir,
            model=model,
            query=args.query,
            query_embedding=query_embedding,
            limit=args.limit,
            filters=filters,
            timing_runs=args.timing_runs,
            fusion=args.fusion,
            vector_candidates=args.vector_candidates,
            keyword_candidates=args.keyword_candidates,
            vector_weight=args.vector_weight,
            keyword_weight=args.keyword_weight,
            rrf_k=args.rrf_k,
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
        "--search-mode",
        choices=["vector", "keyword", "hybrid", "all"],
        default="hybrid",
        help="retrieval mode to run",
    )
    parser.add_argument(
        "--where-kind",
        choices=["scene", "shot"],
        help="optional metadata filter for scene or shot chunks",
    )
    parser.add_argument(
        "--start-time",
        type=float,
        help="optional lower bound for time filtering",
    )
    parser.add_argument(
        "--end-time",
        type=float,
        help="optional upper bound for time filtering",
    )
    parser.add_argument(
        "--time-filter-mode",
        choices=["overlap", "contained"],
        default="overlap",
        help="how to match start_time/end_time intervals when a time filter is present",
    )
    parser.add_argument(
        "--fusion",
        choices=["weighted", "rrf"],
        default="weighted",
        help="hybrid fusion mode",
    )
    parser.add_argument(
        "--vector-candidates",
        type=int,
        help="optional hybrid vector candidate count before fusion",
    )
    parser.add_argument(
        "--keyword-candidates",
        type=int,
        help="optional hybrid keyword candidate count before fusion",
    )
    parser.add_argument(
        "--vector-weight",
        type=float,
        default=0.6,
        help="weighted hybrid vector score weight",
    )
    parser.add_argument(
        "--keyword-weight",
        type=float,
        default=0.4,
        help="weighted hybrid keyword score weight",
    )
    parser.add_argument(
        "--rrf-k",
        type=float,
        default=60.0,
        help="RRF k value when --fusion rrf is selected",
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
        "--chunk-token-limit",
        type=int,
        default=DEFAULT_CHUNK_TOKEN_LIMIT,
        help="approximate token limit for shot section chunks",
    )
    parser.add_argument(
        "--chunk-overlap",
        type=int,
        default=DEFAULT_CHUNK_OVERLAP,
        help="number of sections/sentences to overlap when splitting long shot text",
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
    args = parser.parse_args()
    if (
        args.start_time is not None
        and args.end_time is not None
        and args.start_time > args.end_time
    ):
        parser.error("--start-time must be less than or equal to --end-time")
    return args


def build_records(
    source_path: Path,
    *,
    chunk_token_limit: int = DEFAULT_CHUNK_TOKEN_LIMIT,
    chunk_overlap: int = DEFAULT_CHUNK_OVERLAP,
) -> list[SearchRecord]:
    with source_path.open("r", encoding="utf-8") as file:
        data = json.load(file)

    media_name = scalar_string(data.get("metadata", {}).get("media_name"), "unknown")
    records: list[SearchRecord] = []

    for scene_id, scene in sorted(data.get("scenes", {}).items()):
        records.extend(scene_records(media_name, scene_id, scene))

    shot_to_scene = build_shot_to_scene(data.get("scenes", {}))
    for shot_id, shot in sorted(data.get("shots", {}).items()):
        records.extend(
            shot_records(
                media_name,
                shot_id,
                shot,
                scene_number=shot_to_scene.get(shot_id),
                chunk_token_limit=chunk_token_limit,
                chunk_overlap=chunk_overlap,
            )
        )

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
        documents: list[DocumentInput] = []

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


def build_filter(
    *,
    kind: str | None,
    start_time: float | None,
    end_time: float | None,
    time_filter_mode: str,
) -> Filter | None:
    filters: list[Filter] = []
    if kind:
        filters.append(vk_where.eq("kind", kind))

    time_filter = build_time_filter(
        start_time=start_time,
        end_time=end_time,
        time_filter_mode=time_filter_mode,
    )
    if time_filter is not None:
        filters.append(time_filter)

    if not filters:
        return None
    if len(filters) == 1:
        return filters[0]
    return vk_where.all(*filters)


def build_time_filter(
    *,
    start_time: float | None,
    end_time: float | None,
    time_filter_mode: str,
) -> Filter | None:
    if start_time is None and end_time is None:
        return None

    filters: list[Filter] = []
    if time_filter_mode == "overlap":
        if end_time is not None:
            filters.append(vk_where.range("start_time", lte=end_time))
        if start_time is not None:
            filters.append(vk_where.range("end_time", gte=start_time))
    elif time_filter_mode == "contained":
        if start_time is not None:
            filters.append(vk_where.range("start_time", gte=start_time))
        if end_time is not None:
            filters.append(vk_where.range("end_time", lte=end_time))
    else:
        raise ValueError(f"unsupported time filter mode: {time_filter_mode}")

    if len(filters) == 1:
        return filters[0]
    return vk_where.all(*filters)


def run_searches(
    *,
    index: Index,
    mode: str,
    query: str,
    query_embedding: list[float],
    limit: int,
    filters: Filter | None,
    fusion: str,
    vector_candidates: int | None,
    keyword_candidates: int | None,
    vector_weight: float,
    keyword_weight: float,
    rrf_k: float,
) -> list[SearchRun]:
    modes = ["vector", "keyword", "hybrid"] if mode == "all" else [mode]
    runs: list[SearchRun] = []

    for search_mode in modes:
        start = time.perf_counter()
        if search_mode == "vector":
            hits = index.search(query_embedding, limit=limit, where=filters)
        elif search_mode == "keyword":
            hits = index.keyword_search(query, limit=limit, where=filters)
        elif search_mode == "hybrid":
            if not hasattr(index, "hybrid_search"):
                raise RuntimeError(
                    "installed vectorkit package does not expose Index.hybrid_search; "
                    "rerun scripts/setup-social-network-example.sh or reinstall the "
                    "current Python wrapper wheel"
                )
            hits = index.hybrid_search(
                query,
                query_embedding,
                limit=limit,
                where=filters,
                vector_candidates=vector_candidates,
                keyword_candidates=keyword_candidates,
                fusion=fusion,
                vector_weight=vector_weight,
                keyword_weight=keyword_weight,
                rrf_k=rrf_k,
            )
        else:
            raise ValueError(f"unsupported search mode: {search_mode}")

        runs.append(SearchRun(search_mode, hits, elapsed_ms(start)))

    return runs


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


def scene_records(
    media_name: str,
    scene_id: str,
    scene: dict[str, Any],
) -> list[SearchRecord]:
    records: list[SearchRecord] = []
    metadata_base = scene_metadata(media_name, scene)

    for field_name in SCENE_DESCRIPTION_FIELDS:
        text = normalize_description_text(scene.get(field_name, ""))
        if not validate_text(text):
            continue

        content_type = scene_content_type(field_name)
        document_id = f"scene:{scene_id}:{field_name}"
        metadata = clean_metadata(
            {
                **metadata_base,
                "content_type": content_type,
                "source_field": field_name,
                "chunk_id": document_id,
                "estimated_tokens": estimate_tokens(text),
            }
        )
        records.append(SearchRecord(document_id, text, metadata))

    return records


def shot_records(
    media_name: str,
    shot_id: str,
    shot: dict[str, Any],
    *,
    scene_number: int | None,
    chunk_token_limit: int,
    chunk_overlap: int,
) -> list[SearchRecord]:
    records: list[SearchRecord] = []
    metadata_base = shot_metadata(media_name, shot, scene_number=scene_number)

    for section_name, header, field_name in SHOT_SECTIONS:
        content = shot_section_text(shot.get(field_name), header)
        if not validate_text(content):
            continue

        chunk_base = f"{shot_id}_{section_name}"
        section_chunks = chunk_text_with_overlap(
            content,
            chunk_base,
            token_limit=chunk_token_limit,
            overlap_size=chunk_overlap,
        )

        for chunk_id, chunk_text in section_chunks:
            document_id = f"shot:{chunk_id}"
            metadata = clean_metadata(
                {
                    **metadata_base,
                    "content_type": shot_content_type(section_name),
                    "source_field": field_name,
                    "section": section_name,
                    "chunk_id": chunk_id,
                    "estimated_tokens": estimate_tokens(chunk_text),
                }
            )
            records.append(SearchRecord(document_id, chunk_text, metadata))

    return records


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
    *,
    scene_number: int | None,
) -> dict[str, str | int | float | bool]:
    structure = shot.get("structure", {})
    timing = shot.get("timing", {})
    return clean_metadata(
        {
            "media_name": media_name,
            "kind": "shot",
            "scene_number": scene_number,
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


def normalize_description_text(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False)
    return ""


def shot_section_text(value: Any, header: str) -> str:
    if header == "VIDEO DESCRIPTION":
        body = "" if value is None else str(value).strip()
    elif isinstance(value, (dict, list)):
        body = yaml.dump(value, allow_unicode=True, sort_keys=False)
    else:
        body = normalize_description_text(value)
    if not body:
        return ""
    return f"=== {header} ===\n{body}"


def estimate_tokens(text: str) -> int:
    return len(text) // 4


def chunk_text_with_overlap(
    text: str,
    chunk_id_base: str,
    *,
    token_limit: int,
    overlap_size: int,
) -> list[tuple[str, str]]:
    if estimate_tokens(text) <= token_limit:
        return [(f"{chunk_id_base}_chunk_1", text)]

    if "\n\n" in text:
        sections = text.split("\n\n")
        join_separator = "\n\n"
    elif "\n" in text:
        sections = text.split("\n")
        join_separator = "\n"
    else:
        sections = text.split(". ")
        join_separator = ". "
        sections = [
            section + ". " if index < len(sections) - 1 else section
            for index, section in enumerate(sections)
        ]

    chunks: list[tuple[str, str]] = []
    current_chunk: list[str] = []
    current_tokens = 0
    chunk_number = 1

    for section in sections:
        section_tokens = estimate_tokens(section)

        if current_tokens + section_tokens > token_limit and current_chunk:
            chunk_text = join_separator.join(current_chunk)
            chunks.append((f"{chunk_id_base}_chunk_{chunk_number}", chunk_text))
            chunk_number += 1

            overlap_start = max(0, len(current_chunk) - overlap_size)
            current_chunk = current_chunk[overlap_start:]
            current_tokens = sum(estimate_tokens(item) for item in current_chunk)

        current_chunk.append(section)
        current_tokens += section_tokens

    if current_chunk:
        chunk_text = join_separator.join(current_chunk)
        chunks.append((f"{chunk_id_base}_chunk_{chunk_number}", chunk_text))

    return chunks


def scene_content_type(field_name: str) -> str:
    mapping = {
        "people_description": "people_description",
        "object_description": "object_description",
        "location_description": "location_description",
        "temporal_description": "temporal_description",
        "audio_description": "audio_description",
        "emotions_description": "emotions_description",
        "visual_description": "visual_description",
        "video_description": "video_description",
        "story_description": "video_description",
    }
    return mapping.get(field_name, field_name)


def shot_content_type(section_name: str) -> str:
    mapping = {
        "location": "location_description",
        "temporal": "temporal_description",
        "people": "people_description",
        "objects": "object_description",
        "emotion": "emotions_description",
        "audio": "audio_description",
        "visual": "visual_description",
        "video": "video_description",
    }
    return mapping.get(section_name, "visual_description")


def build_shot_to_scene(scenes: dict[str, Any]) -> dict[str, int]:
    shot_to_scene: dict[str, int] = {}
    for scene in scenes.values():
        structure = scene.get("structure", {}) if isinstance(scene, dict) else {}
        scene_number = structure.get("scene_number")
        for shot_id in structure.get("shots", []):
            if isinstance(shot_id, str) and isinstance(scene_number, int):
                shot_to_scene[shot_id] = scene_number
    return shot_to_scene


def validate_text(text: Any) -> bool:
    return bool(text and isinstance(text, str) and text.strip())


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


def print_hits(
    query: str,
    mode: str,
    hits: list[SearchHit] | list[KeywordHit] | list[HybridHit],
) -> None:
    print(f"\nQuery: {query}")
    print(f"Mode: {mode}")
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
        print_hit_trace(mode, hit)
        print(f"   {hit['text'][:500]}...")


def print_hit_trace(
    mode: str,
    hit: SearchHit | KeywordHit | HybridHit,
) -> None:
    if mode == "keyword":
        terms = hit.get("matched_terms", [])
        print(f"   matched_terms={terms}")
        return

    if mode == "hybrid":
        trace = hit["trace"]
        vector_score = hit.get("vector_score")
        keyword_score = hit.get("keyword_score")
        terms = hit.get("matched_terms", [])
        fusion = trace.get("fusion", {})
        print(
            "   "
            f"vector_score={format_optional_score(vector_score)} "
            f"keyword_score={format_optional_score(keyword_score)} "
            f"matched_terms={terms}"
        )
        print(
            "   "
            f"vector_rank={trace.get('vector_rank')} "
            f"keyword_rank={trace.get('keyword_rank')} "
            f"fusion={fusion}"
        )


def format_optional_score(value: float | None) -> str:
    return "none" if value is None else f"{value:.4f}"


def print_one_shot_timings(
    model_init_ms: float,
    load_ms: float,
    query_embed_ms: float,
    search_runs: list[SearchRun],
) -> None:
    print("\nOne-shot timing:")
    print(f"  model_init_ms: {model_init_ms:.3f}")
    print(f"  load_ms: {load_ms:.3f}")
    print(f"  query_embed_ms: {query_embed_ms:.3f}")
    for run in search_runs:
        print(f"  {run.mode}_search_ms: {run.search_ms:.3f}")
        if run.mode in {"vector", "hybrid"}:
            print(f"  {run.mode}_query_total_ms: {query_embed_ms + run.search_ms:.3f}")


def print_timing_report(
    *,
    index_dir: Path,
    model: TextEmbedding,
    query: str,
    query_embedding: list[float],
    limit: int,
    filters: Filter | None,
    timing_runs: int,
    fusion: str,
    vector_candidates: int | None,
    keyword_candidates: int | None,
    vector_weight: float,
    keyword_weight: float,
    rrf_k: float,
) -> None:
    load_times = []
    query_embed_times = []
    vector_search_times = []
    keyword_search_times = []
    hybrid_search_times = []
    semantic_total_times = []
    hybrid_total_times = []

    for _ in range(timing_runs):
        start = time.perf_counter()
        loaded = Index.load(index_dir)
        load_times.append(elapsed_ms(start))

    index = Index.load(index_dir)
    has_hybrid_search = hasattr(index, "hybrid_search")

    for _ in range(timing_runs):
        start = time.perf_counter()
        vector = embed_one(model, query)
        query_embed_times.append(elapsed_ms(start))

        start = time.perf_counter()
        index.search(vector, limit=limit, where=filters)
        search_ms = elapsed_ms(start)
        semantic_total_times.append(query_embed_times[-1] + search_ms)

        if has_hybrid_search:
            start = time.perf_counter()
            index.hybrid_search(
                query,
                vector,
                limit=limit,
                where=filters,
                vector_candidates=vector_candidates,
                keyword_candidates=keyword_candidates,
                fusion=fusion,
                vector_weight=vector_weight,
                keyword_weight=keyword_weight,
                rrf_k=rrf_k,
            )
            hybrid_ms = elapsed_ms(start)
            hybrid_total_times.append(query_embed_times[-1] + hybrid_ms)

    for _ in range(timing_runs):
        start = time.perf_counter()
        index.search(query_embedding, limit=limit, where=filters)
        vector_search_times.append(elapsed_ms(start))

    for _ in range(timing_runs):
        start = time.perf_counter()
        index.keyword_search(query, limit=limit, where=filters)
        keyword_search_times.append(elapsed_ms(start))

    if has_hybrid_search:
        for _ in range(timing_runs):
            start = time.perf_counter()
            index.hybrid_search(
                query,
                query_embedding,
                limit=limit,
                where=filters,
                vector_candidates=vector_candidates,
                keyword_candidates=keyword_candidates,
                fusion=fusion,
                vector_weight=vector_weight,
                keyword_weight=keyword_weight,
                rrf_k=rrf_k,
            )
            hybrid_search_times.append(elapsed_ms(start))

    print(f"\nAverage timing over {timing_runs} runs:")
    print_stats("load_ms", load_times)
    print_stats("query_embed_ms", query_embed_times)
    print_stats("vector_search_ms", vector_search_times)
    print_stats("keyword_search_ms", keyword_search_times)
    print_stats("semantic_query_total_ms", semantic_total_times)
    if has_hybrid_search:
        print_stats("hybrid_search_ms", hybrid_search_times)
        print_stats("hybrid_query_total_ms", hybrid_total_times)
    else:
        print("  hybrid_search_ms: skipped; installed vectorkit has no hybrid_search")


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
