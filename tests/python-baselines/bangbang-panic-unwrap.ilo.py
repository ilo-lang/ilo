def _ilo_unwrap(r):
    if r[0] == "ok":
        return r[1]
    raise RuntimeError(r[1])

def parse_ok() -> float:
    return _ilo_unwrap(((lambda v: ("ok", float(v)) if isinstance(v, (int, float)) and not isinstance(v, bool) else (lambda s: ("ok", float(s)) if s.strip().replace('.','',1).replace('-','',1).isdigit() else ("err", s))(v))("42")))

def parse_err() -> float:
    return _ilo_unwrap(((lambda v: ("ok", float(v)) if isinstance(v, (int, float)) and not isinstance(v, bool) else (lambda s: ("ok", float(s)) if s.strip().replace('.','',1).replace('-','',1).isdigit() else ("err", s))(v))("abc")))

def mget_hit() -> float:
    m = mset(mmap(), "k", 7)
    return _ilo_unwrap(mget(m, "k"))

def mget_miss() -> float:
    m = mset(mmap(), "k", 7)
    return _ilo_unwrap(mget(m, "missing"))
