cargo build --release
cd ui
bun --bun rsbuild build
cd ..

rm -r target/build
mkdir -p target/build

cp target/release/myousync.exe target/build/
mkdir -p target/build/web
cp -r ui/dist/* target/build/web
