-- Create additional databases
CREATE DATABASE documents_test;

-- Create users
CREATE USER test_user WITH PASSWORD 'test_password';

-- Grant connection privileges to the test user
GRANT ALL PRIVILEGES ON DATABASE documents_test TO test_user;

-- Connect to the DB to set schema-specific privileges
\c documents_test

GRANT USAGE ON SCHEMA public TO test_user;
GRANT CREATE ON SCHEMA public TO test_user;

-- For future tables that will be created
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO test_user;
