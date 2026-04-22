from enum import Enum, IntEnum, auto


class Color(Enum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"


class Level(IntEnum):
    DEBUG = 0
    INFO = 1
    WARN = 2
    ERROR = 3


class Status(Enum):
    PENDING = auto()
    ACTIVE = auto()
    DONE = auto()


def label(c: Color) -> str:
    if c == Color.RED:
        return "r"
    return "x"
