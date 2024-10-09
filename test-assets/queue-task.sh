#!/bin/bash

echo
echo "Inserting sample task into queue"

curl -X "POST" "http://elasticmq:9324/" \
     -H 'X-Amz-Target: AmazonSQS.SendMessage' \
     -H 'Content-Type: application/x-amz-json-1.0' \
     -d $'{
  "QueueUrl": "http://elasticmq:9324/testQueue.fifo/",
  "MessageGroupId": "1",
  "MessageBody": "{\\"job\\": \\"/test-job\\", \\"hello\\": \\"world\\"}",
  "MessageAttributes": {
    "traceId": {
      "DataType": "String",
      "StringValue": "A12345"
    }
  },
  "MessageDeduplicationId": "'"$(date +%s)"'"
}'

echo
