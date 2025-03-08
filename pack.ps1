cd myousync
cargo build --release
cd ..

cd ui
bun install
bun --bun rsbuild build
cd ..

rm -r build
mkdir -p build
mkdir -p build/web

cp myousync/target/release/myousync.exe build/
cp -r ui/dist/* build/web
