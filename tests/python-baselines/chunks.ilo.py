def basic() -> list[list[float]]:
    return chunks(2, [1, 2, 3, 4, 5])

def exact() -> list[list[float]]:
    return chunks(3, [1, 2, 3, 4, 5, 6])

def big() -> list[list[float]]:
    return chunks(10, [1, 2, 3])

def singles() -> list[list[float]]:
    return chunks(1, [1, 2, 3])

def empty() -> list[list[float]]:
    return chunks(2, [])
