from operations import Worker
def run():
    worker=Worker()
    alias=worker
    f=open('x')
    with f:
        value=alias.read(f)
        worker.finish(f)
        return value

def unrelated(x):
    external(x)
