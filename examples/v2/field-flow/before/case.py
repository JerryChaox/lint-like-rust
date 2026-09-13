class Box:
    def __init__(self,f):
        self.f=f
    def close(self):
        self.f.close()
    def read(self):
        return self.f.read()
def run():
    f=open('x')
    b=Box(f)
    b.close()
    return b.read()
