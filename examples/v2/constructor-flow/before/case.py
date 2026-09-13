from operations import Worker
def run():
    f=open('x')
    alias=f
    with f:
        Worker(f)
        return alias.read()
