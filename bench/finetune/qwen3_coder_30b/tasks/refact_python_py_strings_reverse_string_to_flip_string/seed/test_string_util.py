"""Plain-asserts test file. Run with: python3 test_string_util.py"""
import string_util

def main():
    assert string_util.reverse_string("") == ""
    assert string_util.reverse_string("hello") == "olleh"
    assert string_util.reverse_string("a") == "a"

    assert string_util.count_vowels("") == 0
    assert string_util.count_vowels("hello") == 2
    assert string_util.count_vowels("AEIOU") == 5
    assert string_util.count_vowels("xyz") == 0

    assert string_util.is_palindrome("racecar")
    assert string_util.is_palindrome("A man a plan a canal Panama")
    assert not string_util.is_palindrome("hello")
    assert string_util.is_palindrome("")

    print("OK")

if __name__ == "__main__":
    main()
