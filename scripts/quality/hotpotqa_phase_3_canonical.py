"""Canonical JSON encoding shared by the sealed Phase 3b Python tools."""

from __future__ import annotations

import json
import math
from typing import Any


def _trim_fraction(value: str) -> str:
    if "." not in value:
        return value
    trimmed = value.rstrip("0").rstrip(".")
    return "0" if trimmed == "-0" else trimmed


def _scientific_to_plain(mantissa: str, exponent: int) -> str:
    negative = mantissa.startswith("-")
    unsigned = mantissa.removeprefix("-")
    digits = unsigned.replace(".", "")
    decimal = 1 + exponent
    if decimal <= 0:
        plain = f"0.{('0' * -decimal)}{digits}"
    elif decimal >= len(digits):
        plain = f"{digits}{'0' * (decimal - len(digits))}"
    else:
        plain = f"{digits[:decimal]}.{digits[decimal:]}"
    plain = _trim_fraction(plain)
    return f"-{plain}" if negative else plain


def _number(value: int | float) -> str:
    if isinstance(value, int):
        return str(value)
    if not math.isfinite(value):
        raise ValueError("non-finite JSON numbers are forbidden")
    if value == 0:
        return "0"
    text = repr(value).replace("E", "e").replace("e+", "e")
    if "e" not in text:
        return _trim_fraction(text)
    mantissa, exponent_text = text.split("e", 1)
    exponent = int(exponent_text)
    if -6 <= exponent <= 20:
        return _scientific_to_plain(mantissa, exponent)
    sign = "-" if exponent < 0 else ""
    return f"{_trim_fraction(mantissa)}e{sign}{abs(exponent)}"


def _encode(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, (int, float)):
        return _number(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    if isinstance(value, list):
        return f"[{','.join(_encode(item) for item in value)}]"
    if isinstance(value, dict):
        entries = sorted(value.items(), key=lambda item: item[0].encode())
        return "{" + ",".join(
            f"{_encode(key)}:{_encode(item)}" for key, item in entries
        ) + "}"
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def canonical(value: Any) -> bytes:
    """Encode with the same ordering, escaping, and number rules as Rust."""
    return _encode(value).encode()
