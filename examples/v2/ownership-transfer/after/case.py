import io

def run():
    raw=io.BytesIO(b'payload')
    old=raw
    data=old.read()
    text=io.TextIOWrapper(raw,'utf-8')
    text.close()
    return data
