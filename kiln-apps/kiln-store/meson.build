project('kiln-store', version: '0.1.0', license: 'GPL-3.0-or-later', meson_version: '>= 0.60')
pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), 'kiln-store')
themedir = pkgdatadir
conf = configuration_data()
conf.set('pkgdatadir', pkgdatadir)
conf.set('themedir', themedir)
configure_file(input: 'src/kiln-store.in', output: 'kiln-store', configuration: conf, install: true,
               install_dir: get_option('bindir'), install_mode: 'rwxr-xr-x')
configure_file(input: 'src/kiln_store/app.py', output: 'app.py', configuration: conf)
install_data('src/kiln_store/__init__.py', 'src/kiln_store/backends.py', install_dir: join_paths(pkgdatadir, 'kiln_store'))
install_data(meson.current_build_dir() / 'app.py', install_dir: join_paths(pkgdatadir, 'kiln_store'))
install_data('data/kiln-light.css', 'data/kiln-dark.css', install_dir: pkgdatadir)
install_data('data/org.kiln.Store.desktop', install_dir: join_paths(get_option('datadir'), 'applications'))
install_data('data/org.kiln.Store.svg', install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'))
