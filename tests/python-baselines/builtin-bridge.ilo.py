def _ilo_rd(path, fmt=None):
    import os, json, csv, io
    if not os.path.exists(path):
        return ("err", f"{path}: no such file")
    try:
        raw = open(path).read()
        if fmt is None:
            ext = os.path.splitext(path)[1].lstrip('.').lower()
        else:
            ext = fmt
        return ("ok", _ilo_parse_fmt(raw, ext))
    except Exception as e:
        return ("err", str(e))

def _ilo_rdb(s, fmt):
    try:
        return ("ok", _ilo_parse_fmt(s, fmt))
    except Exception as e:
        return ("err", str(e))

def _ilo_parse_fmt(s, fmt):
    import json, csv, io
    if fmt in ("csv", "tsv"):
        sep = '\t' if fmt == "tsv" else ','
        return [row for row in csv.reader(io.StringIO(s), delimiter=sep)]
    if fmt == "json":
        return json.loads(s)
    return s

def digits() -> list[str]:
    return rgx("\\d+", "a1 b22 c333")

def pairs() -> list[list[str]]:
    return rgxall("(\\w+)=(\\d+)", "x=1 y=22 z=333")

def sentence() -> str:
    return ("{} hits across {} files").format(3, 1)

def parsed() -> tuple[str, list[list[str]] | str]:
    return _ilo_rdb("a,1\nb,2", "csv")
