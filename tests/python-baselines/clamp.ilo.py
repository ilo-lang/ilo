def into(x: float, lo: float, hi: float) -> float:
    return clamp(x, lo, hi)

def vol(v: float) -> float:
    return clamp(v, 0, 100)
