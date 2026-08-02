def basic(h: bool) -> float:
    return (1 if h else 0)

def strings(h: bool) -> str:
    return ("yes" if h else "no")

def calc(h: bool) -> float:
    return ((1 + 2) if h else (3 * 4))

def pos(x: float) -> str:
    c = (x > 0)
    return ("pos" if c else "nonpos")

def pick(h: bool) -> float:
    v = (10 if h else 20)
    return v

def arms(h: bool) -> float:
    if h == True:
        return 10
    elif h == False:
        return 20

def unbraced(h: bool) -> float:
    return (1 if h else 0)

def unbraced_t(h: bool) -> str:
    return ("yes" if h else "no")

def unbraced_pick(h: bool) -> float:
    v = (10 if h else 20)
    return v

def unbraced_calc(h: bool) -> float:
    return ((1 + 2) if h else (3 * 4))
