class Counter:
    def __init__(self, start=0):
        self.n = start

    def inc(self):
        self.n += 1
        return self.n
