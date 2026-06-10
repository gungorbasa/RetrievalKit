"""Filter helper constructors for VectorKit searches."""

from __future__ import annotations

from .types import Filter, FilterOperatorSpec, MetadataValue


def eq(field: str, value: MetadataValue) -> Filter:
    return {field: {"$eq": value}}


def ne(field: str, value: MetadataValue) -> Filter:
    return {field: {"$ne": value}}


def in_(field: str, values: list[MetadataValue]) -> Filter:
    return {field: {"$in": values}}


def exists(field: str) -> Filter:
    return {field: {"$exists": True}}


def range(
    field: str,
    *,
    gte: MetadataValue | None = None,
    lte: MetadataValue | None = None,
) -> Filter:
    spec: FilterOperatorSpec = {}
    if gte is not None:
        spec["$gte"] = gte
    if lte is not None:
        spec["$lte"] = lte
    return {field: spec}


def all(*filters: Filter) -> Filter:
    return {"$and": list(filters)}


def any(*filters: Filter) -> Filter:
    return {"$or": list(filters)}
