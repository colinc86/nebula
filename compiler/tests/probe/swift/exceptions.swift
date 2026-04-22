enum BadInput: Error {
    case empty
    case notANumber(String)
}

func parseNum(_ s: String) throws -> Int {
    if s.isEmpty { throw BadInput.empty }
    guard let n = Int(s) else { throw BadInput.notANumber(s) }
    return n
}

func tryParse(_ s: String) -> Int? {
    return try? parseNum(s)
}

func parseOrDie(_ s: String) -> Int {
    return try! parseNum(s)
}

// do / catch
func safeParse(_ s: String) -> Int {
    do {
        return try parseNum(s)
    } catch BadInput.empty {
        return 0
    } catch {
        return -1
    }
}
