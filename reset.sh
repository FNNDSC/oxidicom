#!/bin/bash -e

docker exec chris python manage.py shell -c '
from django.conf import settings
from core.storage import connect_storage
from pacsfiles.models import PACSFile

for pacs_file in PACSFile.objects.all():
    pacs_file.delete()

storage = connect_storage(settings)
for f in storage.ls("SERVICES/PACS"):
    storage.delete_obj(f)
'
