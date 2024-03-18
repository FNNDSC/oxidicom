#!/bin/bash
# Purpose: populate Orthanc with sample data

GITHUB_TARBALLS=(
  https://api.github.com/repos/FNNDSC/SAG-anon/tarball/3d6e850b625e940aab02f0120cf5fb15977216bc
  https://api.github.com/repos/datalad/example-dicom-structural/tarball/f077bcc8d502ce8155507bd758cb3b7ccc887f40
)

until instances=$(curl -sf localhost:8042/instances); do
  sleep 1
done
if [ "$(jq -r length <<< "$instances")" != '0' ]; then
  echo 'Already have data'
  exit 0
fi

tmpdir=$(mktemp -d)
cd $tmpdir

for url in "${GITHUB_TARBALLS[@]}"; do
  echo "Downloading $url"
  curl -fL "$url" | tar xz
done

find -type f -iname '*.dcm' \
  | parallel --bar -j 4 "curl -sfX POST http://localhost:8042/instances -H Expect: -H 'Content-Type: application/dicom' --data-binary @'{}' -o /dev/null"

cd /
rm -rf $tmpdir
