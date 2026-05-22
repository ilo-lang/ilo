def _ilo_unwrap(r):
    if r[0] == "ok":
        return r[1]
    raise RuntimeError(r[1])

def sev(j: str) -> tuple[str, float | str]:
    r = _ilo_unwrap((lambda s: ("ok", __import__('json').loads(s)))(j))
    return r["baseSeverity"]

def url(j: str) -> tuple[str, float | str]:
    r = _ilo_unwrap((lambda s: ("ok", __import__('json').loads(s)))(j))
    return r["gitURL"]

def chained(j: str) -> tuple[str, float | str]:
    r = _ilo_unwrap((lambda s: ("ok", __import__('json').loads(s)))(j))
    return r["baseSeverity"]["label"]
