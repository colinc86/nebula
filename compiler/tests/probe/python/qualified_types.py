import numpy as np
import pandas as pd


def zeros(n: int) -> np.ndarray:
    return np.zeros(n)


def head(df: pd.DataFrame, n: int) -> pd.DataFrame:
    return df.head(n)


def shape(arr: np.ndarray) -> tuple[int, ...]:
    return arr.shape


def column(df: pd.DataFrame, name: str) -> pd.Series:
    return df[name]
