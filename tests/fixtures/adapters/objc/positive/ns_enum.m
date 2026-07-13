#import <Foundation/Foundation.h>

typedef NS_ENUM(NSInteger, Status) {
    StatusOff,
    StatusOn,
};

typedef NS_OPTIONS(NSUInteger, Permissions) {
    PermRead = 1 << 0,
    PermWrite = 1 << 1,
};
