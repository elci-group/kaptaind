#import <Foundation/Foundation.h>

@interface Greeter : NSObject

@property (nonatomic, copy) NSString *name;
@property (nonatomic, assign) NSInteger energy;

- (void)greet:(NSString *)target;
+ (instancetype)sharedGreeter;

@end

@implementation Greeter
- (void)greet:(NSString *)target {
    NSLog(@"Hello %@", target);
}
@end
