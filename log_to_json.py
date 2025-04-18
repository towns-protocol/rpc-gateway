import json
import sys

def convert_log_to_json_array(log_file_path):
    json_objects = []
    
    with open(log_file_path, 'r') as file:
        for line in file:
            line = line.strip()
            if line:  # Skip empty lines
                try:
                    json_obj = json.loads(line)
                    json_objects.append(json_obj)
                except json.JSONDecodeError as e:
                    print(f"Error parsing line: {e}", file=sys.stderr)
    
    return json_objects

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python log_to_json.py <log_file_path>")
        sys.exit(1)
    
    log_file_path = sys.argv[1]
    json_array = convert_log_to_json_array(log_file_path)
    print(json.dumps(json_array, indent=2)) 