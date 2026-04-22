func fetchUser(_ id: Int) async -> String {
    return "user_\(id)"
}

func fetchAll(_ ids: [Int]) async -> [String] {
    var out: [String] = []
    for id in ids {
        out.append(await fetchUser(id))
    }
    return out
}

// concurrent with async let
func parallelFetch(_ a: Int, _ b: Int) async -> (String, String) {
    async let r1 = fetchUser(a)
    async let r2 = fetchUser(b)
    return await (r1, r2)
}

// task group
func fetchMany(_ ids: [Int]) async -> [String] {
    return await withTaskGroup(of: String.self) { group in
        for id in ids {
            group.addTask { await fetchUser(id) }
        }
        var out: [String] = []
        for await r in group { out.append(r) }
        return out
    }
}
