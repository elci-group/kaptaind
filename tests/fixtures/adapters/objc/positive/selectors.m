#import <Foundation/Foundation.h>

@interface Config : NSObject

- (void)setName:(NSString *)name age:(NSInteger)age;
- (void)setTitle:(NSString *)title
          subtitle:(NSString *)subtitle
            active:(BOOL)active;
+ (instancetype)defaultConfig;

@end
