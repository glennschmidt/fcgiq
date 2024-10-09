<?php

sleep(3);

if ($_SERVER['REQUEST_URI'] == '/test-job') {
    $payload = json_decode(file_get_contents('php://input'), flags: JSON_THROW_ON_ERROR);
    header('Content-Type: application/json');
    echo json_encode(['message' => 'Script ran successfully (hello = '.$payload->hello.', trace = '.$_SERVER['TRACE_ID'].')']);
} else {
    header('HTTP/1.1 404 Not Found');
}
