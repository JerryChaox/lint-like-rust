import io

def run():
    raw=io.BytesIO(b'payload')
    old=raw
    text=io.TextIOWrapper(raw,'utf-8')
    external(text)
    data=old.read()
    text.close()
    return data
