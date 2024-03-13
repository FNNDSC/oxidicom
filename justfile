get-data:
    curl -f 'https://zenodo.org/records/3677090/files/0219191_mystudy-0219-1114.tar.gz?download=1' | tar xz
    find 0219191_mystudy-0219-1114 -type f -name "*.dcm.gz" | parallel gzip -d

# TODO code this up as a Rust integration test...
push:
    docker run --rm --net=host -u "$(id -u):$(id -g)" -v $PWD/0219191_mystudy-0219-1114:/data:ro ghcr.io/fnndsc/pfdcm:3.1.22 storescu localhost 11111 /data/dcm -v +sd +r -aet HOSPITALPACS -aec ChRIS

push-one:
    docker run --rm --net=host -u "$(id -u):$(id -g)" -v $PWD/0219191_mystudy-0219-1114:/data:ro ghcr.io/fnndsc/pfdcm:3.1.22 storescu localhost 11111 /data/dcm/12-3-1.dcm -v -aet HOSPITALPACS -aec ChRIS
