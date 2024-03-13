test:
    ./test.sh

clean:
    docker volume rm cargo-oxidicom-target cargo-oxidicom-home

reset:
    ./reset.sh

get-data:
    curl -fL 'https://api.github.com/repos/FNNDSC/SAG-anon/tarball/3d6e850b625e940aab02f0120cf5fb15977216bc' | tar xz
    rm FNNDSC-SAG-anon-3d6e850/LICENSE FNNDSC-SAG-anon-3d6e850/README.md
