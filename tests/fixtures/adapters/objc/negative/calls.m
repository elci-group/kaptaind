#import <Foundation/Foundation.h>

void run(void) {
    NSString *name = @"kaptaind";
    NSInteger count = 3;
    NSLog(@"Hello %@ (%ld)", name, (long)count);
    [name uppercaseString];
    id obj = [NSObject new];
    [obj doThingWithName:name count:count];
}
