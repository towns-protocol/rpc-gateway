import random
import string

def generate_key(length=32):
    # Define the character sets
    uppercase_letters = string.ascii_uppercase
    lowercase_letters = string.ascii_lowercase
    numbers = string.digits
    
    # Combine all character sets
    all_chars = uppercase_letters + lowercase_letters + numbers
    
    # Generate the key
    key = ''.join(random.choice(all_chars) for _ in range(length))
    return key

if __name__ == "__main__":
    # Generate and print a key
    key = generate_key()
    print(key) 