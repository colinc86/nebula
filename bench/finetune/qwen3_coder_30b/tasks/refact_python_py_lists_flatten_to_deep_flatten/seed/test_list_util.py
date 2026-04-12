"""Plain-asserts test file. Run with: python3 test_list_util.py"""
import list_util

def main():
    assert list_util.flatten([[1, 2], [3, 4]]) == [1, 2, 3, 4]
    assert list_util.flatten([]) == []
    assert list_util.flatten([[]]) == []

    assert list_util.chunk([1, 2, 3, 4], 2) == [[1, 2], [3, 4]]
    assert list_util.chunk([1, 2, 3, 4, 5], 2) == [[1, 2], [3, 4], [5]]
    assert list_util.chunk([], 2) == []
    assert list_util.chunk([1, 2, 3], 0) == []

    assert list_util.unique([1, 1, 2, 3, 2]) == [1, 2, 3]
    assert list_util.unique([]) == []
    assert list_util.unique(["a", "b", "a"]) == ["a", "b"]

    print("OK")

if __name__ == "__main__":
    main()
