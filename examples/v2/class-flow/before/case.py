from operations import Worker
def run():
    worker=Worker()
    alias=worker
    f=open('x')
    with f:
        worker.finish(f)
        return alias.read(f)
