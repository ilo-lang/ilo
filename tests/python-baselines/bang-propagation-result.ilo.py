def _ilo_unwrap(r):
    if r[0] == "ok":
        return r[1]
    raise RuntimeError(r[1])

def parse_ok() -> tuple[str, float | str]:
    v = _ilo_unwrap((lambda s: ("ok", float(s)) if s.replace('.','',1).replace('-','',1).isdigit() else ("err", s))("42"))
    return ("ok", v)

def parse_err() -> tuple[str, float | str]:
    v = _ilo_unwrap((lambda s: ("ok", float(s)) if s.replace('.','',1).replace('-','',1).isdigit() else ("err", s))("abc"))
    return ("ok", v)

def fmt_err() -> tuple[str, str | str]:
    v = _ilo_unwrap(dtfmt(99999999999999, "%Y"))
    return ("ok", v)
