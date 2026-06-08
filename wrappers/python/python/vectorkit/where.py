"""Filter helper constructors for VectorKit searches."""

from __future__ import annotations

from typing import Any


def eq(field: str, value: Any) -> dict[str, Any]:
    return {field: {"$eq": value}}


def ne(field: str, value: Any) -> dict[str, Any]:
    return {field: {"$ne": value}}


def in_(field: str, values: list[Any]) -> dict[str, Any]:
    return {field: {"$in": values}}


def exists(field: str) -> dict[str, Any]:
    return {field: {"$exists": True}}


def range(
    field: str,
    *,
    gte: Any | None = None,
    lte: Any | None = None,
) -> dict[str, Any]:
    spec: dict[str, Any] = {}
    if gte is not None:
        spec["$gte"] = gte
    if lte is not None:
        spec["$lte"] = lte
    return {field: spec}


def all(*filters: dict[str, Any]) -> dict[str, Any]:
    return {"$and": list(filters)}


def any(*filters: dict[str, Any]) -> dict[str, Any]:
    return {"$or": list(filters)}
