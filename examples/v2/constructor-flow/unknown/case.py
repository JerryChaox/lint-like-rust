from operations import Worker
def run():
    f=open('x')
    alias=f
    with f:
        value=alias.read()
        Worker(f)
        return value
