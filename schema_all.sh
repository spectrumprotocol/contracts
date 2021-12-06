cd contracts
dirs=($(find . -maxdepth 2 -type d \( ! -name . \)))
for dir in "${dirs[@]}"; do
  cd $dir
  cargo schema
  cd ..
done
cd ..
