def flatten(nested):
    out = []
    for sub in nested:
        for x in sub:
            out.append(x)
    return out
