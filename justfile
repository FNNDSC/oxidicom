test:
    ./run.sh test

clean:
    docker volume rm cargo-oxidicom-target cargo-oxidicom-home

reset:
    ./reset.sh

dev:
    ./run.sh run

kill:
    docker kill cargo-chris-scp
    docker container rm cargo-chris-scp

get-data:
    curl -fL 'https://api.github.com/repos/FNNDSC/SAG-anon/tarball/3d6e850b625e940aab02f0120cf5fb15977216bc' | tar xz
    rm FNNDSC-SAG-anon-3d6e850/LICENSE FNNDSC-SAG-anon-3d6e850/README.md

    chrs --username "" --cube https://cube.chrisproject.org/api/v1/ download plugininstance/214 plinst214
    mkdir greenEyes-anat
    find plinst214 -type f -name '*.dcm' -exec mv '{}' greenEyes-anat \;
    rm -r plinst214

    mkdir example_data
    mv FNNDSC-SAG-anon-3d6e850 greenEyes-anat example_data

push-sag-anon:
    storescu localhost 11112 example_data/FNNDSC-SAG-anon-3d6e850 -v -aet HOSPITALPACS -aec ChRIS --verbose --scan-directories --recurse

push-greenEyes:
    storescu localhost 11112 example_data/greenEyes-anat -aet HOSPITALPACS -aec ChRIS --verbose --scan-directories --recurse

push-one:
    storescu localhost 11112 example_data/FNNDSC-SAG-anon-3d6e850/0188-1.3.12.2.1107.5.2.19.45152.2013030808105567563785463.dcm -aet HOSPITALPACS -aec ChRIS --verbose
