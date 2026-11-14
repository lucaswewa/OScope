"""ex-omext's dummy extension: a greeting node, which leaves the image as it
is. ex-omext opens its extensions without Python, so it only lists it; a
server that runs Python extensions (P26) runs it."""

from oscope_ext import node


@node("hello.greet", category="example")
def greet(image):
    """Leaves the image as it is: a greeting."""
    return image
