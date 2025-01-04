project('libaudiowire', 'c')

compiler = meson.get_compiler('c')

src = ['src/ringbuf.c', 'src/portaudio.c']
deps = compiler.find_library('portaudio', required: true)
test_deps = compiler.find_library('ws2_32', required: true)
inc = include_directories('include')

lib = library(
    'audiowire',
    src,
    include_directories: inc,
    dependencies: deps,
    install: true,
)

audiowire_test = executable(
    'audiowire-test',
    'tests/audiowire_test.c',
    include_directories: inc,
    link_with: lib,
    dependencies: test_deps,
)
ringbuf_test = executable(
    'ringbuf-test',
    'tests/ringbuf_test.c',
    include_directories: inc,
    link_with: lib,
)

install_headers('include/audiowire.h', 'include/ringbuf.h', subdir: 'audiowire')

test('ringbuf test', ringbuf_test)
test('audiowire test', audiowire_test)