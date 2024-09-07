#!/bin/bash
# Purpose: populate Orthanc with sample data

GITHUB_TARBALLS=(
  https://api.github.com/repos/FNNDSC/SAG-anon/tarball/3d6e850b625e940aab02f0120cf5fb15977216bc
  https://api.github.com/repos/datalad/example-dicom-structural/tarball/f077bcc8d502ce8155507bd758cb3b7ccc887f40
)

until instances=$(curl -sf http://localhost:8042/instances); do
  printf .
  sleep 1
done
echo

get_instance_counts () {
  curl -sf http://localhost:8042/series \
    | jq -r '.[]' \
    | xargs -I _ sh -c '
        curl -sf http://localhost:8042/series/_ \
          | jq -r ".MainDicomTags.SeriesInstanceUID + \" \" + (.Instances | length | tostring)"' \
    | sort
}

series_has_data () {
  echo "$1" | grep -qF "$2 $3" || (echo "Error: $2 has wrong number of DICOM instances"; return 1)
}

check_has_data () {
  local data="$(get_instance_counts)"
  echo "$data"
  series_has_data "$data" '1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652' 384
  series_has_data "$data" '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0'      192
}

if check_has_data > /dev/null; then
  echo "Already have data"
  exit 0
fi

tmpdir=$(mktemp -d)
cd $tmpdir

for url in "${GITHUB_TARBALLS[@]}"; do
  echo "Downloading $url"
  curl -fL "$url" | tar xz
done

find -type f -iname '*.dcm' \
  | xargs -I _ curl -sfX POST http://localhost:8042/instances -H Expect: -H 'Content-Type: application/dicom' --data-binary @_ -o /dev/null

cd /
rm -rf $tmpdir

check_has_data
