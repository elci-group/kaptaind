CREATE FUNCTION user_count() RETURNS integer AS $$
SELECT count(*) FROM users;
$$ LANGUAGE SQL;

CREATE PROCEDURE refresh_stats() LANGUAGE plpgsql AS $$
BEGIN
    ANALYZE users;
END;
$$;

CREATE SEQUENCE ticket_seq;

CREATE TRIGGER audit_users AFTER INSERT ON users
FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
