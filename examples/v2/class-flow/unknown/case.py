from operations import Worker
def run():
    worker=Worker()
    alias=worker
    external()
    f=open('x')
    with f:
        value=alias.read(f)
        worker.finish(f)
        return value
