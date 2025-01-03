project('libaudiowire', 'c')

compiler = meson.get_compiler('c')

src = 'src/portaudio.c'
deps = compiler.find_library('portaudio')
inc = include_directories('include')

lib = library(
    'audiowire',
    src,
    include_directories: inc,
    dependencies: deps,
    install: true,
)
util = library(
    'audiowireutil',
    'src/ringbuf.c',
    include_directories: inc,
    install: true,
)

audiowire_test = executable(
    'audiowire-test',
    'tests/audiowire_test.c',
    include_directories: inc,
    link_with: [lib, util],
)
ringbuf_test = executable(
    'ringbuf-test',
    'tests/ringbuf_test.c',
    include_directories: inc,
    link_with: util,
)
install_headers('include/audiowire.h')

test('ringbuf test', ringbuf_test)
test('audiowire test', audiowire_test)