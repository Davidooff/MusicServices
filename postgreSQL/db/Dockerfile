# Use official Postgres image as base
FROM postgres:latest

# Copy your initialization scripts into the place Postgres auto-runs them
COPY init/*.sql /docker-entrypoint-initdb.d/

# (Optional) copy any custom config:
# COPY postgres.conf /etc/postgresql/postgresql.conf
# and then add:
# CMD ["postgres", "-c", "config_file=/etc/postgresql/postgresql.conf"]




#sudo docker run -d --name music-db-container -p 5432:5432 -e POSTGRES_USER=admin -e POSTGRES_PASSWORD=admin -e POSTGRES_DB=musicdb -v db_data:/var/lib/postgresql/data postgres:latest

#sudo docker run -d --name music-db   -p 5432:5432   -e POSTGRES_USER=admin   -e POSTGRES_PASSWORD=admin   -e POSTGRES_DB=musicdb   -v db_data:/var/lib/postgresql/data postgres:latest