import io

def run():
    raw = io.BytesIO(b'payload')
    view = raw.getbuffer()
    data = raw.read()
    view.release()
    raw.close()
    return data
