def nth(xs: list[float], i: float) -> float:
    return at(xs, i)

def last(xs: list[float]) -> float:
    return at(xs, -1)

def penultimate(xs: list[float]) -> float:
    return at(xs, -2)
