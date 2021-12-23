cd contracts
dirs=($(find . -maxdepth 1 -type d \( ! -name . \)))
for dir in "${dirs[@]}"; do
  cd $dir
  cargo schema
  cd ..
done

cd farms
dirs=($(find . -maxdepth 1 -type d \( ! -name . \)))
for dir in "${dirs[@]}"; do
  cd $dir
  cargo schema
  cd ..
done
cd ..
cd ..
