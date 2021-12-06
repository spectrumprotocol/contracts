cd contracts
cd core
dirs=($(find . -maxdepth 1 -type d \( ! -name . \)))
for dir in "${dirs[@]}"; do
  cd $dir
  cargo schema
  cd ..
done
cd ..

cd farms
dirs=($(find . -maxdepth 1 -type d \( ! -name . \)))
for dir in "${dirs[@]}"; do
  cd $dir
  cargo schema
  cd ..
done
cd ..
cd ..
