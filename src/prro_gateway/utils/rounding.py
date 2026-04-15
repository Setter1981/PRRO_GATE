"""NBU rounding per Постанова №115 від 08.09.2025.

From 01.10.2025 cash sums are rounded to nearest 50 kopecks.
"""
from __future__ import annotations


def round_to_50_kopecks(total_sum: int) -> int:
    """Round total_sum (kopecks) to nearest 50-kopeck boundary.

    Returns rounded_sum in kopecks.
    """
    remainder = total_sum % 100
    if remainder == 0 or remainder == 50:
        return total_sum
    elif 1 <= remainder <= 24:
        return total_sum - remainder
    elif 25 <= remainder <= 49:
        return total_sum - remainder + 50
    elif 51 <= remainder <= 74:
        return total_sum - remainder + 50
    else:  # 75-99
        return total_sum - remainder + 100


__all__ = ['round_to_50_kopecks']
