def gcd(a, b):
    if a < 0: a = -a
    if b < 0: b = -b
    while b != 0:
        a, b = b, a % b
    return a
