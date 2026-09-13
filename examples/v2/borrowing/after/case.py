import io

def run():
    raw = io.BytesIO(b'payload')
    view = raw.getbuffer()
    view.release()
    data = raw.read()
    raw.close()
    return data
