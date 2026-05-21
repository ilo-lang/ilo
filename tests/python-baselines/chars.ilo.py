def ascii() -> list[str]:
    return chars("abc")

def unicode() -> list[str]:
    return chars("café")

def empty() -> list[str]:
    return chars("")

def count() -> float:
    return len(chars("hello"))
