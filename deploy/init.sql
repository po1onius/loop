\c imitv

create table users (
    user_id UUID PRIMARY KEY DEFAULT uuidv7(),
    username VARCHAR(100) NOT NULL,
    account VARCHAR(100) UNIQUE NOT NULL,
    pwd VARCHAR(60) NOT NULL
);

create table refresh_token (
    user_id UUID PRIMARY KEY DEFAULT uuidv7(),
    token_num BIGINT DEFAULT 0
);

