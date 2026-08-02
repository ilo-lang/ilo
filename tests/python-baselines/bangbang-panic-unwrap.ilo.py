def _ilo_unwrap(r):
    if r[0] == "ok":
        return r[1]
    raise RuntimeError(r[1])

def parse_ok() -> float:
    return _ilo_unwrap((lambda s: ("ok", float(s)) if s.replace('.','',1).replace('-','',1).isdigit() else ("err", s))("42"))

def parse_err() -> float:
    return _ilo_unwrap((lambda s: ("ok", float(s)) if s.replace('.','',1).replace('-','',1).isdigit() else ("err", s))("abc"))

def mget_hit() -> float:
    m = mset(mmap(), "k", 7)
    return _ilo_unwrap(mget(m, "k"))

def mget_miss() -> float:
    m = mset(mmap(), "k", 7)
    return _ilo_unwrap(mget(m, "missing"))
