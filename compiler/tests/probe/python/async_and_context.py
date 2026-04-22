import asyncio


async def fetch(url):
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as resp:
            return await resp.text()


async def many(urls):
    return await asyncio.gather(*[fetch(u) for u in urls])


def read_file(path):
    with open(path, "r") as f:
        return f.read()


def try_parse(s):
    try:
        return int(s)
    except ValueError as e:
        print(f"bad: {e}")
        return None
    finally:
        print("done")
